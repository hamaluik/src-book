//! Page header rendering.
//!
//! Adds a file path or section header at the top of each page with an underline.
//! Headers are positioned on the left for odd pages and right for even pages.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use anyhow::Result;
use owned_ttf_parser::AsFaceRef;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;

/// Render a page header with the given text and an underline.
pub fn render_header<S: ToString>(
    config: &PDF,
    doc: &Document,
    font_ids: &FontIds,
    page: &mut Page,
    text: S,
) -> Result<()> {
    let subheading_size = Pt(config.font_size_subheading_pt);

    // add the current file to the top of each page
    // figure out where the header should go
    let header = text.to_string();
    let mut header_start =
        layout::baseline_start(&page, &doc.fonts[font_ids.regular], subheading_size);
    let is_even = doc.page_order.len() % 2 == 0;
    if is_even {
        header_start.0 = page.content_box.x2
            - layout::width_of_text(&header, &doc.fonts[font_ids.regular], subheading_size);
    }

    // figure out the underline
    let (line_offset, line_thickness) = doc.fonts[font_ids.regular]
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

    // add a line below the header
    let mut content = Content::new();
    content
        .set_stroke_gray(0.75)
        .set_line_cap(LineCapStyle::ButtCap)
        .set_line_width(*line_thickness)
        .move_to(*page.content_box.x1, *header_start.1 + *line_offset)
        .line_to(*page.content_box.x2, *header_start.1 + *line_offset)
        .stroke();
    page.add_content(content);

    // write the header
    page.add_span(SpanLayout {
        text: header,
        font: SpanFont {
            id: font_ids.regular,
            size: subheading_size,
        },
        colour: Colour::new_grey(0.25),
        coords: header_start,
    });

    Ok(())
}
