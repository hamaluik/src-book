//! PDF rendering orchestration.
//!
//! Coordinates rendering of all book sections: title page, frontmatter, source files,
//! images, commit history, and table of contents. Manages hierarchical PDF bookmarks
//! for navigation.
//!
//! Frontmatter files (README, LICENSE, etc.) are rendered first with their own
//! bookmark section, providing readers with project context before diving into code.

mod commits;
mod header;
mod images;
mod source_file;
mod table_of_contents;
mod title_page;

use crate::sinks::pdf::booklet::render_booklet;
use crate::sinks::pdf::config::{RenderStats, PDF};
use crate::sinks::pdf::fonts::{FontIds, LoadedFonts};
use crate::source::Source;
use anyhow::{Context, Result};
use pdf_gen::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub const PAGE_SIZE: (Pt, Pt) = (Pt(5.5 * 72.0), Pt(8.5 * 72.0));

impl PDF {
    pub fn render(&self, source: &Source) -> Result<RenderStats> {
        // load fonts based on configuration
        let fonts = LoadedFonts::load(&self.font)
            .with_context(|| format!("Failed to load font '{}'", self.font))?;

        let ss: SyntaxSet = bincode::deserialize(crate::highlight::SERIALIZED_SYNTAX)
            .expect("can deserialize syntaxes");
        let ts: ThemeSet = bincode::deserialize(crate::highlight::SERIALIZED_THEMES)
            .expect("can deserialize themes");

        let mut doc = Document::default();
        let font_ids = FontIds {
            regular: doc.add_font(fonts.regular),
            bold: doc.add_font(fonts.bold),
            italic: doc.add_font(fonts.italic),
            bold_italic: doc.add_font(fonts.bold_italic),
        };

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

        title_page::render(&self, &mut doc, &font_ids, source)
            .with_context(|| "Failed to render title page")?;
        // add a blank page after the title page so we start on the right
        doc.add_page(Page::new(PAGE_SIZE, None));

        doc.add_bookmark(None, "Title", 0).borrow_mut().bolded();
        doc.add_bookmark(None, "Table of Contents", 2)
            .borrow_mut()
            .italicized();

        let mut frontmatter_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut source_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut page_offset = doc.page_order.len();

        // render frontmatter files first if present
        if !source.frontmatter_files.is_empty() {
            let frontmatter_bookmark =
                doc.add_bookmark(None, "Frontmatter", doc.page_order.len());
            frontmatter_bookmark.borrow_mut().bolded();

            for file in source.frontmatter_files.iter() {
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
                        let page_index = images::render(&self, &mut doc, &font_ids, file)?;
                        let file_name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string());
                        doc.add_bookmark(Some(frontmatter_bookmark.clone()), file_name, page_index);
                    }
                    _ => {
                        if let Some(page_index) = source_file::render(
                            &self,
                            &mut doc,
                            &font_ids,
                            file,
                            &ss,
                            &ts.themes[self.theme.name()],
                        )
                        .with_context(|| {
                            format!("Failed to render frontmatter file {}!", file.display())
                        })? {
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
            }
        }

        let source_code_bookmark = doc.add_bookmark(None, "Source Files", doc.page_order.len());
        {
            source_code_bookmark.borrow_mut().bolded();
        }

        // track folder bookmarks for hierarchical structure
        let mut folder_bookmarks: HashMap<PathBuf, Rc<RefCell<OutlineEntry>>> = HashMap::new();

        for file in source.source_files.iter() {
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
                    let page_index = images::render(&self, &mut doc, &font_ids, file)?;
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
                    if let Some(page_index) = source_file::render(
                        &self,
                        &mut doc,
                        &font_ids,
                        file,
                        &ss,
                        &ts.themes[self.theme.name()],
                    )
                    .with_context(|| {
                        format!("Failed to render source file {}!", file.display())
                    })? {
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
        }

        let commit_list = source
            .commits()
            .with_context(|| "Failed to get commits for repository")?;
        let commit_page_index = commits::render(&self, &mut doc, &font_ids, commit_list)
            .with_context(|| "Failed to render commit history")?;
        if let Some(commit_page) = commit_page_index {
            doc.add_bookmark(None, "Commit History", commit_page);
        }

        let num_toc_pages = table_of_contents::render(
            &self,
            &mut doc,
            &font_ids,
            page_offset,
            frontmatter_pages,
            source_pages,
            commit_page_index,
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

        // add page numbers
        let page_number_size = Pt(self.font_size_small_pt);
        for (pi, page_id) in doc.page_order.iter().skip(page_offset).enumerate() {
            let text = format!("{}", pi + 1);
            let page = doc.pages.get_mut(*page_id).expect("page exists");
            let coords: (Pt, Pt) = if pi % 2 == 0 {
                (
                    page.content_box.x2
                        - layout::width_of_text(&text, &doc.fonts[font_ids.regular], page_number_size),
                    In(0.25).into(),
                )
            } else {
                (In(0.25).into(), In(0.25).into())
            };
            page.add_span(SpanLayout {
                text,
                font: SpanFont {
                    id: font_ids.regular,
                    size: page_number_size,
                },
                colour: Colour::new_grey(0.25),
                coords,
            });
        }

        let page_count = doc.page_order.len();

        // generate booklet PDF if configured
        let booklet_sheets = if let Some(booklet_path) = &self.booklet_outfile {
            let sheets = render_booklet(&self, &doc, &font_ids, booklet_path)
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

fn offset_bookmark_page_indices(
    items: &mut [Rc<RefCell<OutlineEntry>>],
    offset_amount: usize,
) {
    for item in items {
        let has_children = !item.borrow().children.is_empty();
        if has_children {
            offset_bookmark_page_indices(&mut item.borrow_mut().children, offset_amount)
        }
        item.borrow_mut().page_index += offset_amount;
    }
}
