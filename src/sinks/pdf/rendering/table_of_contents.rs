//! Table of contents with clickable links.
//!
//! Generates a TOC listing all source files and commit history with page numbers.
//! Each entry links to its corresponding page within the document.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::PAGE_SIZE;
use anyhow::Result;
use owned_ttf_parser::AsFaceRef;
use pdf_gen::id_arena_crate::Id;
use pdf_gen::layout::Margins;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A tree node for the table of contents.
struct TocEntry {
    name: String,
    page: Option<usize>,
    children: Vec<TocEntry>,
}

impl TocEntry {
    fn new_file(name: String, page: usize) -> Self {
        Self {
            name,
            page: Some(page),
            children: Vec::new(),
        }
    }

    fn new_folder(name: String) -> Self {
        Self {
            name,
            page: None,
            children: Vec::new(),
        }
    }

    /// Returns the minimum page number in this subtree (for folder links).
    fn min_page(&self) -> Option<usize> {
        if let Some(p) = self.page {
            return Some(p);
        }
        self.children.iter().filter_map(|c| c.min_page()).min()
    }
}

/// Builds a tree from a flat mapping of paths to page numbers.
fn build_tree(source_pages: HashMap<PathBuf, usize>) -> TocEntry {
    let mut root = TocEntry::new_folder(String::new());

    // sort by page number for consistent ordering
    let mut entries: Vec<_> = source_pages.into_iter().collect();
    entries.sort_by_key(|(_, page)| *page);

    for (path, page) in entries {
        insert_path(&mut root, &path, page);
    }

    root
}

/// Inserts a file path into the tree, creating intermediate folders as needed.
fn insert_path(root: &mut TocEntry, path: &Path, page: usize) {
    let components: Vec<_> = path.components().collect();
    let mut current = root;

    for (i, component) in components.iter().enumerate() {
        let name = component.as_os_str().to_string_lossy().to_string();
        let is_last = i == components.len() - 1;

        if is_last {
            // insert the file
            current.children.push(TocEntry::new_file(name, page));
        } else {
            // find or create the folder
            let folder_name = format!("{}/", name);
            let pos = current
                .children
                .iter()
                .position(|c| c.name == folder_name && c.page.is_none());

            if let Some(idx) = pos {
                current = &mut current.children[idx];
            } else {
                current.children.push(TocEntry::new_folder(folder_name));
                let last_idx = current.children.len() - 1;
                current = &mut current.children[last_idx];
            }
        }
    }
}

/// A flattened TOC entry ready for rendering.
struct FlatEntry {
    prefix: String,
    name: String,
    page: usize,
}

/// Flattens the tree into a list of entries with tree-drawing prefixes.
fn flatten_tree(root: &TocEntry) -> Vec<FlatEntry> {
    let mut result = Vec::new();

    // add "Source Code" as root entry
    if let Some(min_page) = root.min_page() {
        result.push(FlatEntry {
            prefix: String::new(),
            name: "Source Code".to_string(),
            page: min_page,
        });
    }

    flatten_children(&root.children, &mut result, "  ".to_string());
    result
}

fn flatten_children(children: &[TocEntry], result: &mut Vec<FlatEntry>, prefix: String) {
    for (i, child) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };

        let page = child.page.or_else(|| child.min_page()).unwrap_or(0);

        result.push(FlatEntry {
            prefix: format!("{}{}", prefix, connector),
            name: child.name.clone(),
            page,
        });

        if !child.children.is_empty() {
            let child_prefix = if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            flatten_children(&child.children, result, child_prefix);
        }
    }
}

/// Render the table of contents.
///
/// Inserts TOC pages at `skip_pages` position and returns the number of pages added.
/// Pages are padded to an even count to maintain booklet alignment.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    skip_pages: usize,
    source_pages: HashMap<PathBuf, usize>,
    git_history_page: Option<usize>,
) -> Result<usize> {
    let contents_size = Pt(config.font_size_heading_pt);
    let entry_size = Pt(config.font_size_body_pt);
    let subheading_size = Pt(config.font_size_subheading_pt);

    let height_contents = doc.fonts[font_ids.bold].line_height(contents_size);
    let height_entry = doc.fonts[font_ids.regular].line_height(entry_size);
    let descent_entry = doc.fonts[font_ids.regular].descent(entry_size);

    let entry_font = SpanFont {
        id: font_ids.regular,
        size: entry_size,
    };

    // TODO: deal with when we have more than 1 toc page!
    // probably have to pre-calculate how many toc pages we're going to generate
    let mut num_toc_pages = 1;
    if num_toc_pages % 2 == 1 {
        num_toc_pages += 1;
    }

    // figure out the underline
    let (underline_offset, underline_thickness) = doc.fonts[font_ids.regular]
        .face
        .as_face_ref()
        .underline_metrics()
        .map(|metrics| {
            let scaling = subheading_size
                / doc.fonts[font_ids.regular].face.as_face_ref().units_per_em() as f32;
            (
                scaling * metrics.position as f32,
                scaling * metrics.thickness as f32,
            )
        })
        .unwrap_or_else(|| (Pt(-2.0), Pt(0.5)));

    // build tree structure and flatten for rendering
    let tree = build_tree(source_pages);
    let flat_entries = flatten_tree(&tree);

    let mut entries: Vec<(String, usize)> = flat_entries
        .into_iter()
        .map(|e| (format!("{}{}", e.prefix, e.name), e.page))
        .collect();

    if let Some(git_history_page) = git_history_page {
        entries.push(("Commit History".to_string(), git_history_page - skip_pages));
    }

    let mut pages: Vec<Page> = Vec::default();
    while !entries.is_empty() {
        let mut page = Page::new(PAGE_SIZE, Some(Margins::all(In(0.5))));

        let start = if pages.is_empty() {
            layout::baseline_start(&page, &doc.fonts[font_ids.bold], contents_size)
        } else {
            layout::baseline_start(&page, &doc.fonts[font_ids.regular], entry_size)
        };

        let (x, mut y) = start;
        if pages.is_empty() {
            page.add_span(SpanLayout {
                text: "Contents".to_string(),
                font: SpanFont {
                    id: font_ids.bold,
                    size: contents_size,
                },
                colour: colours::BLACK,
                coords: (x, y),
            });
            y -= height_contents;
        }

        'page: loop {
            if y < page.content_box.y1 + descent_entry || entries.is_empty() {
                break 'page;
            }

            let entry = entries.remove(0);
            let entry_width = layout::width_of_text(
                &format!("{} ", entry.0),
                &doc.fonts[font_ids.regular],
                entry_size,
            );
            let pagenum = format!("{}", entry.1 + 1); // page numbering is 0-indexed, add 1 to make it 1-indexed
            let pagenum_width =
                layout::width_of_text(&pagenum, &doc.fonts[font_ids.regular], entry_size);

            let mut underline = Content::new();
            underline
                .set_stroke_gray(0.75)
                .set_line_cap(LineCapStyle::ButtCap)
                .set_line_width(*underline_thickness)
                .move_to(*page.content_box.x1 + *entry_width, *y + *underline_offset)
                .line_to(
                    *page.content_box.x2
                        - *layout::width_of_text(
                            &format!(" {}", pagenum),
                            &doc.fonts[font_ids.regular],
                            entry_size,
                        ),
                    *y + *underline_offset,
                )
                .stroke();
            page.add_content(underline);

            page.add_span(SpanLayout {
                text: entry.0,
                font: entry_font,
                colour: colours::BLACK,
                coords: (x, y),
            });
            page.add_span(SpanLayout {
                text: pagenum,
                font: entry_font,
                colour: colours::BLACK,
                coords: (page.content_box.x2 - pagenum_width, y),
            });

            page.add_intradocument_link_by_index(
                Rect {
                    x1: page.content_box.x1,
                    x2: page.content_box.x2,
                    y1: y,
                    y2: y + doc.fonts[font_ids.regular].ascent(entry_size),
                },
                entry.1 + skip_pages + num_toc_pages,
            );

            y -= height_entry;
        }

        pages.push(page);
    }

    // add a blank page after the contents to keep the booklet even
    if pages.len() % 2 == 1 {
        pages.push(Page::new(PAGE_SIZE, None));
    }

    let added_page_count = pages.len();
    // Add pages to the arena and collect their IDs
    let page_ids: Vec<Id<Page>> = pages.into_iter().map(|p| doc.pages.alloc(p)).collect();
    // Insert the IDs into page_order at the correct position
    for (i, page_id) in page_ids.into_iter().enumerate() {
        doc.page_order.insert(skip_pages + i, page_id);
    }

    Ok(added_page_count)
}
