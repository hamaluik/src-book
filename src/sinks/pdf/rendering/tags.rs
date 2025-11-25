//! Git tags appendix rendering.
//!
//! Displays all tags with their commit info, optionally including tagger
//! and message for annotated tags.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::source::Tag;
use anyhow::Result;
use pdf_gen::layout::Margins;
use pdf_gen::*;

/// Result of rendering the tags appendix section.
pub struct TagsRenderResult {
    /// Page index of the first content page, or None if no tags.
    pub first_page: Option<usize>,
    /// Whether a blank page was inserted for recto alignment.
    pub blank_inserted: bool,
}

/// Render the tags appendix section.
///
/// Returns render result with first page index and blank page info.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    tags: Vec<Tag>,
) -> Result<TagsRenderResult> {
    if tags.is_empty() {
        return Ok(TagsRenderResult {
            first_page: None,
            blank_inserted: false,
        });
    }

    let small_size = Pt(config.fonts.small_pt);
    let subheading_size = Pt(config.fonts.subheading_pt);

    // convert tags to text spans
    let mut text: Vec<(String, Colour, SpanFont)> = Vec::with_capacity(tags.len() * 8 + 1);

    // section title
    let heading_font = SpanFont {
        id: font_ids.bold,
        size: Pt(config.fonts.heading_pt),
    };
    text.push((
        format!("Tags ({} tags)\n\n", tags.len()),
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

    // colours
    let tag_name_colour = Colour::new_rgb_bytes(38, 139, 210); // blue
    let hash_colour = Colour::new_rgb_bytes(143, 63, 113); // magenta
    let summary_colour = Colour::new_rgb_bytes(40, 40, 40); // dark grey
    let date_colour = Colour::new_rgb_bytes(121, 116, 14); // olive
    let author_colour = Colour::new_rgb_bytes(7, 102, 120); // teal
    let message_colour = Colour::new_rgb_bytes(60, 56, 54); // brown-grey

    for tag in tags.into_iter() {
        // tag name (bold blue)
        text.push((tag.name.clone(), tag_name_colour, span_font_bold));

        // arrow and short commit hash
        text.push((
            format!(" → {}", &tag.commit_hash[..8.min(tag.commit_hash.len())]),
            hash_colour,
            span_font_normal,
        ));

        // commit summary
        if let Some(summary) = &tag.commit_summary {
            text.push((format!(" {}", summary), summary_colour, span_font_normal));
        }
        text.push(("\n".to_string(), colours::WHITE, span_font_normal));

        // commit date
        let date_str = jiff::fmt::rfc2822::to_string(&tag.commit_date)
            .unwrap_or_else(|_| tag.commit_date.to_string());
        text.push((
            format!("         {}\n", date_str),
            date_colour,
            span_font_normal,
        ));

        // for annotated tags: show tagger and message
        if tag.is_annotated {
            if let Some(tagger) = &tag.tagger {
                text.push((
                    format!("         Tagged by: {}\n", tagger),
                    author_colour,
                    span_font_normal,
                ));
            }

            if let Some(tag_date) = &tag.tag_date {
                let tag_date_str = jiff::fmt::rfc2822::to_string(tag_date)
                    .unwrap_or_else(|_| tag_date.to_string());
                text.push((
                    format!("         Tag date:  {}\n", tag_date_str),
                    date_colour,
                    span_font_normal,
                ));
            }

            if let Some(message) = &tag.message {
                // indent message lines
                let indented_message = message
                    .lines()
                    .map(|line| format!("         {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                text.push((
                    format!("{}\n", indented_message),
                    message_colour,
                    span_font_normal,
                ));
            }
        }

        // blank line between tags
        text.push(("\n".to_string(), colours::WHITE, span_font_normal));
    }

    // render into pages
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

    Ok(TagsRenderResult {
        first_page,
        blank_inserted,
    })
}
