//! Git commit history rendering.
//!
//! Displays commits with hash, summary, date, author, and optional body text.
//! Commits are rendered in the order provided (typically newest first).
//! Optionally displays tag badges inline with commits.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::source::Commit;
use anyhow::Result;
use pdf_gen::layout::Margins;
use pdf_gen::*;
use std::collections::HashMap;

/// Result of rendering the commit history section.
pub struct CommitRenderResult {
    /// Page index of the first content page, or None if no commits.
    pub first_page: Option<usize>,
    /// Whether a blank page was inserted for recto alignment.
    pub blank_inserted: bool,
}

/// Render the commit history section.
///
/// If `tags_by_commit` is provided and non-empty, tags pointing to each commit
/// are rendered as `[tag_name]` badges after the commit hash.
///
/// Returns render result with first page index and blank page info.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    commits: Vec<Commit>,
    tags_by_commit: Option<&HashMap<String, Vec<String>>>,
) -> Result<CommitRenderResult> {
    let small_size = Pt(config.fonts.small_pt);
    let subheading_size = Pt(config.fonts.subheading_pt);

    // convert the commits to a series of text spans
    let mut text: Vec<(String, Colour, SpanFont)> = Vec::with_capacity(commits.len() * 6 + 1);

    // section title
    let heading_font = SpanFont {
        id: font_ids.bold,
        size: Pt(config.fonts.heading_pt),
    };
    text.push((
        format!("Commit History ({} commits)\n\n", commits.len()),
        colours::BLACK,
        heading_font,
    ));

    let span_font_normal = SpanFont {
        id: font_ids.regular,
        size: small_size,
    };
    let span_font_bold = SpanFont {
        id: font_ids.bold,
        size: small_size,
    };

    // tag badge colour (blue)
    let tag_colour = Colour::new_rgb_bytes(38, 139, 210);

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

        // render inline tag badges if enabled
        if let Some(tags_map) = tags_by_commit {
            if let Some(tags) = tags_map.get(&hash) {
                for tag_name in tags {
                    text.push((format!(" [{}]", tag_name), tag_colour, span_font_bold));
                }
            }
        }

        if let Some(summary) = summary {
            text.push((
                format!(" {}\n", summary),
                Colour::new_rgb_bytes(40, 40, 40),
                span_font_normal,
            ));
        }
        let date_str = jiff::fmt::rfc2822::to_string(&date).unwrap_or_else(|_| date.to_string());
        text.push((
            format!("         {}\n", date_str),
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
    let mut blank_inserted = false;

    while !text.is_empty() {
        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.page_order.len().saturating_sub(1));
        let page_size = config.page_size();

        // insert a blank page so we open to the correct side (recto)
        if first_page.is_none() && doc.page_order.len() % 2 == 1 {
            doc.add_page(Page::new(page_size, Some(margins.clone())));
            blank_inserted = true;
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

        layout::layout_text_naive(doc, &mut page, start, &mut text, wrap_width, bbox);
        let page_id = doc.add_page(page);
        if first_page.is_none() {
            first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
        }
    }

    Ok(CommitRenderResult {
        first_page,
        blank_inserted,
    })
}
