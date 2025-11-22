//! Title page rendering.
//!
//! Creates a centred title page with the book title and author list sorted by prominence.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::PAGE_SIZE;
use crate::source::Source;
use anyhow::Result;
use pdf_gen::*;

/// Render the title page with book title and authors.
pub fn render(config: &PDF, doc: &mut Document, font_ids: &FontIds, source: &Source) -> Result<()> {
    let size_title = Pt(config.font_size_title_pt);
    let size_by = Pt(config.font_size_small_pt);
    let size_author = Pt(config.font_size_body_pt);
    const SPACING: Pt = Pt(72.0 * 0.5);

    let page_size = PAGE_SIZE;
    let descent_title = doc.fonts[font_ids.bold].descent(size_title);

    let title = source.title.clone().unwrap_or("untitled".to_string());
    let mut authors = source.authors.clone();
    authors.sort();
    let authors: Vec<String> = authors.iter().map(ToString::to_string).collect();

    let height_title = doc.fonts[font_ids.bold].line_height(size_title);
    let height_by = doc.fonts[font_ids.regular].line_height(size_by);
    let height_author = doc.fonts[font_ids.regular].line_height(size_author);
    let height_total = height_title
        + descent_title
        + height_by
        + (height_author * authors.len() as f32)
        + (SPACING * 2.0);

    let mut page = Page::new(page_size, None);

    let mut y: Pt = (page_size.1 + height_total) / 2.0;

    let x = (page_size.0 - layout::width_of_text(&title, &doc.fonts[font_ids.bold], size_title))
        / 2.0;
    page.add_span(SpanLayout {
        text: title,
        font: SpanFont {
            id: font_ids.bold,
            size: size_title,
        },
        colour: colours::BLACK,
        coords: (x, y),
    });
    y -= height_title + SPACING + descent_title;

    let x = (page_size.0 - layout::width_of_text("- by -", &doc.fonts[font_ids.regular], size_by))
        / 2.0;
    page.add_span(SpanLayout {
        text: "- by -".to_string(),
        font: SpanFont {
            id: font_ids.bold,
            size: size_by,
        },
        colour: colours::BLACK,
        coords: (x, y),
    });
    y -= height_by + SPACING;

    for author in authors.into_iter() {
        let x = (page_size.0
            - layout::width_of_text(&author, &doc.fonts[font_ids.regular], size_author))
            / 2.0;
        page.add_span(SpanLayout {
            text: author,
            font: SpanFont {
                id: font_ids.bold,
                size: size_author,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_author;
    }

    doc.add_page(page);
    Ok(())
}
