//! PDF rendering orchestration.
//!
//! Coordinates rendering of all book sections: title page, frontmatter, source files,
//! images, commit history, and table of contents. Manages hierarchical PDF bookmarks
//! for navigation.
//!
//! ## Two-Phase Rendering Architecture
//!
//! Source file rendering uses a two-phase approach to enable future parallelisation:
//!
//! 1. **Phase 1 (parallelisable)**: Text files are rendered to `Vec<Page>` without
//!    modifying the document. Each file produces an independent [`source_file::RenderResult`]
//!    containing its pages and bookmark title. This phase can be parallelised with rayon
//!    by changing `.iter().map()` to `.par_iter().map()`.
//!
//! 2. **Phase 2 (sequential)**: Pages are inserted into the document in the correct order,
//!    and bookmarks are created using the returned `Id<Page>` handles. This phase must
//!    remain sequential because it mutates the document.
//!
//! Image files are handled separately because they require document mutation to add
//! images to the arena, so they're rendered inline during phase 2.
//!
//! ## ID-Based Bookmarks
//!
//! Bookmarks use `Id<Page>` references rather than page indices. This means bookmark
//! targets remain stable even when pages are inserted later (e.g., the table of contents
//! is rendered after content but inserted before it). The pdf-gen library resolves these
//! IDs to actual indices during document serialization.
//!
//! ## Document Metadata
//!
//! PDF document properties (title, author, subject, keywords, creator) are set from
//! the source configuration and PDF settings. These appear in PDF viewers under
//! "Document Properties" or similar. The creator field identifies src-book as the
//! generating tool.
//!
//! ## Content Rendering
//!
//! Frontmatter files (README, LICENSE, etc.) are rendered first with their own
//! bookmark section, providing readers with project context before diving into code.
//!
//! The render function accepts a progress bar from the caller, updating it with the
//! current file name and incrementing after each file is processed. This provides
//! visual feedback during long renders of large repositories.
//!
//! ## Cross-Document Resources
//!
//! Image file paths are tracked in an [`ImagePathMap`] during rendering so that
//! booklet generation can reload images into its separate document. See the
//! [`crate::sinks::pdf::booklet`] module for details on why this is necessary.
//!
//! Page metadata ([`PageMetadata`]) is collected for each content page during rendering,
//! tracking which source file each page belongs to. After all content is rendered,
//! headers and footers are applied via [`header_footer::render_headers_and_footers()`],
//! which uses this metadata to populate template placeholders like `{file}`.

mod colophon;
mod commits;
mod header_footer;
mod hex_dump;
mod images;
mod source_file;
mod table_of_contents;
mod tags;
mod title_page;

pub use header_footer::PageMetadata;

use crate::sinks::pdf::booklet::render_booklet;
use crate::sinks::pdf::config::{RenderStats, Section, PDF};
use crate::sinks::pdf::fonts::{FontIds, LoadedFonts};
use crate::source::Source;
use anyhow::{Context, Result};
use indicatif::ProgressBar;
use pdf_gen::id_arena_crate::Id;
use pdf_gen::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Maps image arena indices to their original file paths.
/// Used by booklet rendering to reload images into the booklet document.
pub type ImagePathMap = HashMap<usize, PathBuf>;

impl PDF {
    pub fn render(&self, source: &Source, progress: &ProgressBar) -> Result<RenderStats> {
        // load fonts based on configuration
        let fonts = LoadedFonts::load(&self.font)
            .with_context(|| format!("Failed to load font '{}'", self.font))?;

        let (ss, _): (SyntaxSet, _) = bincode::serde::decode_from_slice(
            crate::highlight::SERIALIZED_SYNTAX,
            bincode::config::standard(),
        )
        .expect("can deserialize syntaxes");
        let (ts, _): (ThemeSet, _) = bincode::serde::decode_from_slice(
            crate::highlight::SERIALIZED_THEMES,
            bincode::config::standard(),
        )
        .expect("can deserialize themes");

        let mut doc = Document::default();
        let font_ids = FontIds {
            regular: doc.add_font(fonts.regular),
            bold: doc.add_font(fonts.bold),
            italic: doc.add_font(fonts.italic),
            bold_italic: doc.add_font(fonts.bold_italic),
        };

        // track image paths for booklet rendering
        let mut image_paths: ImagePathMap = HashMap::new();

        let mut info = Info::default();
        if let Some(title) = &source.title {
            info.title(title);
        }
        let authors = source
            .authors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(" ");
        if !authors.trim().is_empty() {
            info.author(authors);
        }
        if let Some(subject) = self.subject_opt() {
            info.subject(subject);
        }
        if let Some(keywords) = self.keywords_opt() {
            info.keywords(keywords);
        }
        info.creator(concat!("src-book v", env!("CARGO_PKG_VERSION")));
        doc.set_info(info);

        title_page::render(self, &mut doc, &font_ids, source, &mut image_paths)
            .with_context(|| "Failed to render title page")?;

        // render colophon if enabled (before the blank page)
        let commits_for_stats = source.commits().unwrap_or_default();
        let colophon_stats = colophon::compute_stats(source, &commits_for_stats);
        let colophon_page_count =
            colophon::render(self, &mut doc, &font_ids, source, &colophon_stats)
                .with_context(|| "Failed to render colophon page")?;

        // add a blank page after title/colophon so we start on the right (if odd page count)
        let pages_so_far = 1 + colophon_page_count; // title + colophon
        if pages_so_far % 2 == 1 {
            doc.add_page(Page::new(self.page_size(), None));
        }

        // bookmark the first page (title page)
        let title_page_id = doc.page_order[0];
        doc.add_bookmark(None, "Title", title_page_id)
            .borrow_mut()
            .bolded();

        // TOC will be inserted later, so we'll create its bookmark after rendering

        let mut frontmatter_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut source_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut page_offset = doc.page_order.len();
        // track metadata for each content page (for header/footer rendering)
        let mut page_metadata: Vec<PageMetadata> = Vec::new();
        // track page counts within each section for section-specific numbering
        let mut frontmatter_page_count: usize = 0;
        let mut source_page_count: usize = 0;
        let mut commit_history_page_count: usize = 0;

        // render frontmatter files first if present
        let mut first_frontmatter_page_id = None;
        if !source.frontmatter_files.is_empty() {
            for file in source.frontmatter_files.iter() {
                let file_name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| file.display().to_string());
                progress.set_message(file_name.clone());

                frontmatter_pages.insert(file.clone(), doc.page_order.len() - page_offset);

                match file
                    .extension()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .to_str()
                    .unwrap_or_default()
                {
                    "png" | "svg" | "bmp" | "ico" | "jpg" | "jpeg" | "webp" | "avif" | "tga"
                    | "tiff" => {
                        let page_id =
                            images::render(self, &mut doc, &font_ids, file, &mut image_paths)?;
                        // images are single pages
                        page_metadata.push(
                            PageMetadata::new(Section::Frontmatter, frontmatter_page_count)
                                .with_file(file.display().to_string()),
                        );
                        frontmatter_page_count += 1;
                        if first_frontmatter_page_id.is_none() {
                            first_frontmatter_page_id = Some(page_id);
                        }
                    }
                    _ => {
                        let result = source_file::render(
                            self,
                            &doc,
                            &font_ids,
                            file,
                            &ss,
                            &ts.themes[self.theme.name()],
                        )
                        .with_context(|| {
                            format!("Failed to render frontmatter file {}!", file.display())
                        })?;

                        // insert pages and get their IDs
                        let page_ids = doc.add_pages(result.pages);

                        // track metadata for each page rendered
                        let file_display = file.display().to_string();
                        for _ in &page_ids {
                            page_metadata.push(
                                PageMetadata::new(Section::Frontmatter, frontmatter_page_count)
                                    .with_file(file_display.clone()),
                            );
                            frontmatter_page_count += 1;
                        }

                        // create bookmark if we have pages
                        if let Some(&first_page_id) = page_ids.first() {
                            if first_frontmatter_page_id.is_none() {
                                first_frontmatter_page_id = Some(first_page_id);
                            }
                        }
                    }
                }

                progress.inc(1);
            }
        }

        // create frontmatter bookmark now that we know the first page
        // (we don't add child bookmarks to frontmatter, but the bookmark itself is in the outline)
        let _frontmatter_bookmark = if let Some(first_page_id) = first_frontmatter_page_id {
            let bm = doc.add_bookmark(None, "Frontmatter", first_page_id);
            bm.borrow_mut().bolded();
            Some(bm)
        } else {
            None
        };

        // source code bookmark created lazily when we encounter the first source file
        let mut source_code_bookmark: Option<Rc<RefCell<OutlineEntry>>> = None;

        // track folder bookmarks for hierarchical structure
        let mut folder_bookmarks: HashMap<PathBuf, Rc<RefCell<OutlineEntry>>> = HashMap::new();

        // separate source files into images (need doc mutation) and text files (can parallelize)
        let (_image_files, text_files): (Vec<_>, Vec<_>) =
            source.source_files.iter().partition(|file| {
                matches!(
                    file.extension()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .to_str()
                        .unwrap_or_default(),
                    "png"
                        | "svg"
                        | "bmp"
                        | "ico"
                        | "jpg"
                        | "jpeg"
                        | "webp"
                        | "avif"
                        | "tga"
                        | "tiff"
                )
            });

        // phase 1: render text files (can be parallelized in future)
        let mut rendered_text_files: HashMap<PathBuf, source_file::RenderResult> = {
            text_files
                .iter()
                .map(|file| {
                    progress.set_message(
                        file.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string()),
                    );
                    let result = source_file::render(
                        self,
                        &doc,
                        &font_ids,
                        file,
                        &ss,
                        &ts.themes[self.theme.name()],
                    )
                    .with_context(|| format!("Failed to render source file {}!", file.display()))?;
                    progress.inc(1);
                    Ok(((*file).clone(), result))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        // phase 2: add all files to document in original order, creating bookmarks
        for file in source.source_files.iter() {
            let file_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.display().to_string());

            source_pages.insert(file.clone(), doc.page_order.len() - page_offset);

            let is_image = matches!(
                file.extension()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .to_str()
                    .unwrap_or_default(),
                "png" | "svg" | "bmp" | "ico" | "jpg" | "jpeg" | "webp" | "avif" | "tga" | "tiff"
            );

            if is_image {
                // images are rendered directly (need doc for image arena)
                let page_id = images::render(self, &mut doc, &font_ids, file, &mut image_paths)?;
                page_metadata.push(
                    PageMetadata::new(Section::Source, source_page_count)
                        .with_file(file.display().to_string()),
                );
                source_page_count += 1;

                if source_code_bookmark.is_none() {
                    let bm = doc.add_bookmark(None, "Source Code", page_id);
                    bm.borrow_mut().bolded();
                    source_code_bookmark = Some(bm);
                }

                let parent_bookmark = get_or_create_folder_bookmark(
                    &mut doc,
                    &mut folder_bookmarks,
                    source_code_bookmark.as_ref().unwrap(),
                    file,
                    page_id,
                );
                doc.add_bookmark(Some(parent_bookmark), file_name, page_id);
            } else {
                // take the pre-rendered result for this file
                let result = rendered_text_files
                    .remove(file)
                    .expect("file was rendered in phase 1");

                // insert pages and get their IDs
                let page_ids = doc.add_pages(result.pages);

                let file_display = file.display().to_string();
                for _ in &page_ids {
                    page_metadata.push(
                        PageMetadata::new(Section::Source, source_page_count)
                            .with_file(file_display.clone()),
                    );
                    source_page_count += 1;
                }

                if let Some(&first_page_id) = page_ids.first() {
                    if source_code_bookmark.is_none() {
                        let bm = doc.add_bookmark(None, "Source Code", first_page_id);
                        bm.borrow_mut().bolded();
                        source_code_bookmark = Some(bm);
                    }

                    let parent_bookmark = get_or_create_folder_bookmark(
                        &mut doc,
                        &mut folder_bookmarks,
                        source_code_bookmark.as_ref().unwrap(),
                        file,
                        first_page_id,
                    );
                    doc.add_bookmark(Some(parent_bookmark), result.bookmark_title, first_page_id);
                }
            }
        }

        progress.finish_with_message("Files rendered");

        // track pages before commit rendering to count commit pages
        let pages_before_commits = doc.page_order.len();

        // load tags if inline tags are enabled
        let tags_by_commit = if self.inline_tags.enabled {
            Some(
                source
                    .tags_by_commit()
                    .with_context(|| "Failed to get tags for repository")?,
            )
        } else {
            None
        };

        let commit_list = source
            .commits()
            .with_context(|| "Failed to get commits for repository")?;
        let commit_result = commits::render(
            self,
            &mut doc,
            &font_ids,
            commit_list,
            tags_by_commit.as_ref(),
        )
        .with_context(|| "Failed to render commit history")?;
        if let Some(commit_page) = commit_result.first_page {
            doc.add_bookmark(None, "Commit History", commit_page);
        }

        // track commit pages, marking blank recto-alignment page separately
        let commit_total_pages = doc.page_order.len() - pages_before_commits;
        if commit_result.blank_inserted {
            // first page is blank for recto alignment - skip numbering
            page_metadata.push(PageMetadata::new(Section::CommitHistory, 0).skip_numbering());
        }
        // content pages get sequential numbering starting at 0
        let commit_content_pages = if commit_result.blank_inserted {
            commit_total_pages.saturating_sub(1)
        } else {
            commit_total_pages
        };
        for _ in 0..commit_content_pages {
            page_metadata.push(PageMetadata::new(
                Section::CommitHistory,
                commit_history_page_count,
            ));
            commit_history_page_count += 1;
        }

        // render tags appendix if enabled
        let pages_before_tags = doc.page_order.len();
        let tags_result = if self.tags_appendix.enabled {
            let tag_list = source
                .tags(self.tags_appendix.order)
                .with_context(|| "Failed to get tags for repository")?;
            let result = tags::render(self, &mut doc, &font_ids, tag_list)
                .with_context(|| "Failed to render tags appendix")?;
            if let Some(tags_page) = result.first_page {
                doc.add_bookmark(None, "Tags", tags_page);
            }
            result
        } else {
            tags::TagsRenderResult {
                first_page: None,
                blank_inserted: false,
            }
        };

        // track tags pages, marking blank recto-alignment page separately
        let tags_total_pages = doc.page_order.len() - pages_before_tags;
        if tags_result.blank_inserted {
            // first page is blank for recto alignment - skip numbering
            page_metadata.push(PageMetadata::new(Section::Tags, 0).skip_numbering());
        }
        let tags_content_pages = if tags_result.blank_inserted {
            tags_total_pages.saturating_sub(1)
        } else {
            tags_total_pages
        };
        for i in 0..tags_content_pages {
            page_metadata.push(PageMetadata::new(Section::Tags, i));
        }

        let num_toc_pages = table_of_contents::render(
            self,
            &mut doc,
            &font_ids,
            page_offset,
            frontmatter_pages,
            source_pages,
            commit_result.first_page,
            tags_result.first_page,
            commit_content_pages,
        )
        .with_context(|| "Failed to render table of contents")?;
        page_offset += num_toc_pages;

        // note: with Id<Page>-based bookmarks, no offset adjustment is needed
        // IDs remain stable when pages are inserted; resolution happens during write()

        // render headers and footers on all content pages
        let title = source.title.as_deref();
        header_footer::render_headers_and_footers(
            self,
            &mut doc,
            &font_ids,
            page_offset,
            &page_metadata,
            title,
        );

        let page_count = doc.page_order.len();

        // generate booklet PDF if configured
        let booklet_sheets = if let Some(booklet_path) = self.booklet_outfile_path() {
            let sheets = render_booklet(self, source, &doc, &font_ids, &image_paths, &booklet_path)
                .with_context(|| "Failed to render booklet PDF")?;
            Some(sheets)
        } else {
            None
        };

        let file =
            std::fs::File::create(&self.outfile).with_context(|| "Failed to create output file")?;
        let mut file = std::io::BufWriter::new(file);
        doc.write(&mut file)
            .with_context(|| "Failed to render PDF")?;

        Ok(RenderStats {
            page_count,
            booklet_sheets,
        })
    }
}

/// Get or create folder bookmarks for all ancestor directories of a file path,
/// returning the immediate parent folder's bookmark.
fn get_or_create_folder_bookmark(
    doc: &mut Document,
    folder_bookmarks: &mut HashMap<PathBuf, Rc<RefCell<OutlineEntry>>>,
    root_bookmark: &Rc<RefCell<OutlineEntry>>,
    file_path: &Path,
    page_id: Id<Page>,
) -> Rc<RefCell<OutlineEntry>> {
    let parent = match file_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => return root_bookmark.clone(),
    };

    // collect all ancestor paths that need bookmarks
    let mut ancestors: Vec<&Path> = Vec::new();
    let mut current = parent;
    while !current.as_os_str().is_empty() {
        if !folder_bookmarks.contains_key(current) {
            ancestors.push(current);
        }
        current = match current.parent() {
            Some(p) => p,
            None => break,
        };
    }

    // create bookmarks from root to leaf (reverse order)
    for ancestor in ancestors.into_iter().rev() {
        let parent_bookmark = match ancestor.parent() {
            Some(p) if !p.as_os_str().is_empty() => folder_bookmarks
                .get(p)
                .cloned()
                .unwrap_or_else(|| root_bookmark.clone()),
            _ => root_bookmark.clone(),
        };

        // use just the folder name with trailing slash for display
        let folder_name = ancestor
            .file_name()
            .map(|n| format!("{}/", n.to_string_lossy()))
            .unwrap_or_else(|| format!("{}/", ancestor.display()));

        let bookmark = doc.add_bookmark(Some(parent_bookmark), folder_name, page_id);
        folder_bookmarks.insert(ancestor.to_path_buf(), bookmark);
    }

    folder_bookmarks
        .get(parent)
        .cloned()
        .unwrap_or_else(|| root_bookmark.clone())
}
