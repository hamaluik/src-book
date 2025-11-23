//! Git commit history rendering.
//!
//! Displays commits with hash, summary, date, author, and optional body text.
//! Commits are rendered in the order provided (typically newest first).

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::header;
use crate::source::Commit;
use anyhow::Result;
use pdf_gen::layout::Margins;
use pdf_gen::*;

/// Render the commit history section.
///
/// Returns the page index of the first page, or None if there were no commits.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    commits: Vec<Commit>,
) -> Result<Option<usize>> {
    let small_size = Pt(config.font_size_small_pt);
    let subheading_size = Pt(config.font_size_subheading_pt);

    // convert the commits to a series of text spans
    let mut text: Vec<(String, Colour, SpanFont)> = Vec::with_capacity(commits.len() * 6);

    let span_font_normal = SpanFont {
        id: font_ids.regular,
        size: small_size,
    };
    let span_font_bold = SpanFont {
        id: font_ids.bold,
        size: small_size,
    };

    for commit in commits.into_iter() {
        let Commit {
            author,
            summary,
            body,
            date,
            hash,
        } = commit;

        text.push((
            hash.chars().take(8).collect(),
            Colour::new_rgb_bytes(143, 63, 113),
            span_font_bold,
        ));
        if let Some(summary) = summary {
            text.push((
                format!(" {}\n", summary),
                Colour::new_rgb_bytes(40, 40, 40),
                span_font_normal,
            ));
        }
        text.push((
            format!("         {}\n", date.to_rfc2822()),
            Colour::new_rgb_bytes(121, 116, 14),
            span_font_normal,
        ));
        text.push((
            format!("         {}\n", author),
            Colour::new_rgb_bytes(7, 102, 120),
            span_font_normal,
        ));
        if let Some(body) = body {
            text.push((
                format!("         {}\n", body),
                Colour::new_rgb_bytes(60, 56, 54),
                span_font_normal,
            ));
        }
        text.push(("\n".to_string(), colours::WHITE, span_font_normal));
    }

    // and render it into pages
    let wrap_width =
        layout::width_of_text("         ", &doc.fonts[font_ids.bold], span_font_bold.size);
    let mut first_page = None;
    while !text.is_empty() {
        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.page_order.len().saturating_sub(1));
        let page_size = config.page_size();

        // insert a blank page so we open to the correct side
        if first_page.is_none() && doc.page_order.len() % 2 == 1 {
            doc.add_page(Page::new(page_size, Some(margins.clone())));
        }

        let mut page = Page::new(page_size, Some(margins));
        let start = layout::baseline_start(&page, &doc.fonts[font_ids.bold], span_font_bold.size);
        let start = (
            start.0,
            start.1
                - (doc.fonts[font_ids.bold].ascent(span_font_bold.size)
                    - doc.fonts[font_ids.regular].descent(subheading_size))
                - In(0.125).into(),
        );
        let bbox = page.content_box;

        // don't start a page with empty lines
        while let Some(span) = text.first() {
            if span.0 == "\n" {
                text.remove(0);
            } else {
                break;
            }
        }
        if text.is_empty() {
            break;
        }

        header::render_header(config, doc, font_ids, &mut page, "Commit History")?;
        layout::layout_text_natural(doc, &mut page, start, &mut text, wrap_width, bbox);
        let page_id = doc.add_page(page);
        if first_page.is_none() {
            first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
        }
    }

    Ok(first_page)
}
