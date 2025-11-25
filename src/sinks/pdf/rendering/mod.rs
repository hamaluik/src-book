//! PDF rendering orchestration.
//!
//! Coordinates rendering of all book sections: title page, frontmatter, source files,
//! images, commit history, and table of contents. Manages hierarchical PDF bookmarks
//! for navigation.
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

        doc.add_bookmark(None, "Title", 0).borrow_mut().bolded();
        // TOC bookmark index: title (1) + colophon pages + blank page (if added)
        let toc_bookmark_index = doc.page_order.len();
        doc.add_bookmark(None, "Table of Contents", toc_bookmark_index)
            .borrow_mut()
            .italicized();

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
        if !source.frontmatter_files.is_empty() {
            let frontmatter_bookmark = doc.add_bookmark(None, "Frontmatter", doc.page_order.len());
            frontmatter_bookmark.borrow_mut().bolded();

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
                        let page_index =
                            images::render(self, &mut doc, &font_ids, file, &mut image_paths)?;
                        // images are single pages
                        page_metadata.push(
                            PageMetadata::new(Section::Frontmatter, frontmatter_page_count)
                                .with_file(file.display().to_string()),
                        );
                        frontmatter_page_count += 1;
                        let file_name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string());
                        doc.add_bookmark(Some(frontmatter_bookmark.clone()), file_name, page_index);
                    }
                    _ => {
                        let result = source_file::render(
                            self,
                            &mut doc,
                            &font_ids,
                            file,
                            &ss,
                            &ts.themes[self.theme.name()],
                        )
                        .with_context(|| {
                            format!("Failed to render frontmatter file {}!", file.display())
                        })?;

                        // track metadata for each page rendered
                        let file_display = file.display().to_string();
                        for _ in 0..result.page_count {
                            page_metadata.push(
                                PageMetadata::new(Section::Frontmatter, frontmatter_page_count)
                                    .with_file(file_display.clone()),
                            );
                            frontmatter_page_count += 1;
                        }

                        if let Some(page_index) = result.first_page {
                            let file_name = file
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| file.display().to_string());
                            doc.add_bookmark(
                                Some(frontmatter_bookmark.clone()),
                                file_name,
                                page_index,
                            );
                        }
                    }
                }

                progress.inc(1);
            }
        }

        let source_code_bookmark = doc.add_bookmark(None, "Source Files", doc.page_order.len());
        {
            source_code_bookmark.borrow_mut().bolded();
        }

        // track folder bookmarks for hierarchical structure
        let mut folder_bookmarks: HashMap<PathBuf, Rc<RefCell<OutlineEntry>>> = HashMap::new();

        for file in source.source_files.iter() {
            let file_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.display().to_string());
            progress.set_message(file_name);

            source_pages.insert(file.clone(), doc.page_order.len() - page_offset);

            // render an image or source file depending on its extension
            match file
                .extension()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .to_str()
                .unwrap_or_default()
            {
                "png" | "svg" | "bmp" | "ico" | "jpg" | "jpeg" | "webp" | "avif" | "tga"
                | "tiff" => {
                    let page_index =
                        images::render(self, &mut doc, &font_ids, file, &mut image_paths)?;
                    // images are single pages
                    page_metadata.push(
                        PageMetadata::new(Section::Source, source_page_count)
                            .with_file(file.display().to_string()),
                    );
                    source_page_count += 1;
                    let parent_bookmark = get_or_create_folder_bookmark(
                        &mut doc,
                        &mut folder_bookmarks,
                        &source_code_bookmark,
                        file,
                        page_index,
                    );
                    let file_name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.display().to_string());
                    doc.add_bookmark(Some(parent_bookmark), file_name, page_index);
                }
                _ => {
                    let result = source_file::render(
                        self,
                        &mut doc,
                        &font_ids,
                        file,
                        &ss,
                        &ts.themes[self.theme.name()],
                    )
                    .with_context(|| format!("Failed to render source file {}!", file.display()))?;

                    // track metadata for each page rendered
                    let file_display = file.display().to_string();
                    for _ in 0..result.page_count {
                        page_metadata.push(
                            PageMetadata::new(Section::Source, source_page_count)
                                .with_file(file_display.clone()),
                        );
                        source_page_count += 1;
                    }

                    if let Some(page_index) = result.first_page {
                        let parent_bookmark = get_or_create_folder_bookmark(
                            &mut doc,
                            &mut folder_bookmarks,
                            &source_code_bookmark,
                            file,
                            page_index,
                        );
                        let file_name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string());
                        doc.add_bookmark(Some(parent_bookmark), file_name, page_index);
                    }
                }
            }

            progress.inc(1);
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

        // adjust the page numbering of all our source file bookmarks because we inserted a TOC ahead of them
        for entry in doc.outline.entries.iter_mut().skip(2) {
            entry.borrow_mut().page_index += num_toc_pages;
            if !entry.borrow().children.is_empty() {
                offset_bookmark_page_indices(&mut entry.borrow_mut().children, num_toc_pages);
            }
        }

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
    page_index: usize,
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

        let bookmark = doc.add_bookmark(Some(parent_bookmark), folder_name, page_index);
        folder_bookmarks.insert(ancestor.to_path_buf(), bookmark);
    }

    folder_bookmarks
        .get(parent)
        .cloned()
        .unwrap_or_else(|| root_bookmark.clone())
}

fn offset_bookmark_page_indices(items: &mut [Rc<RefCell<OutlineEntry>>], offset_amount: usize) {
    for item in items {
        let has_children = !item.borrow().children.is_empty();
        if has_children {
            offset_bookmark_page_indices(&mut item.borrow_mut().children, offset_amount)
        }
        item.borrow_mut().page_index += offset_amount;
    }
}
