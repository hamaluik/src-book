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
use std::path::PathBuf;

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

    let mut entries: Vec<(String, usize)> = source_pages
        .into_iter()
        .map(|(path, pi)| (path.display().to_string(), pi))
        .collect();
    if let Some(git_history_page) = git_history_page {
        entries.push(("Commit History".to_string(), git_history_page - skip_pages));
    }
    entries.sort_by_key(|(_, p)| *p);

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
