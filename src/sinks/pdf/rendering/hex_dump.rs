//! Hex dump rendering for binary files.
//!
//! When enabled via `render_binary_hex`, binary files are rendered as coloured hex pairs
//! instead of a simple placeholder. This allows readers to inspect the actual contents
//! of compiled binaries, object files, or other non-text files included in the repository.
//!
//! ## Colouring Scheme
//!
//! Colours are derived from the selected syntax theme to maintain visual consistency
//! with syntax-highlighted source code. Byte categories map to theme scopes:
//!
//! - **Null bytes (0x00)**: comment colour (muted, typically grey)
//! - **ASCII printable (0x21-0x7E)**: string colour (typically green/warm)
//! - **ASCII whitespace (tab, newline, etc.)**: keyword colour (typically blue/purple)
//! - **ASCII control chars**: punctuation colour
//! - **Non-ASCII (0x80-0xFF)**: constant colour (typically orange/cyan)
//!
//! This categorisation is inspired by [hexyl](https://github.com/sharkdp/hexyl).
//!
//! ## Caveats
//!
//! Rendering binary files as hex dramatically increases PDF size and rendering time.
//! A single 64KB binary file produces thousands of individually-coloured text spans.
//! Use the `binary_hex_max_bytes` config option to limit the amount rendered per file.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::{header, PAGE_SIZE};
use pdf_gen::layout::Margins;
use pdf_gen::*;
use std::path::Path;

/// Byte categories for hex colouring, similar to hexyl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteCategory {
    /// Null byte (0x00)
    Null,
    /// ASCII printable characters (0x20-0x7E)
    AsciiPrintable,
    /// ASCII whitespace (tab, newline, carriage return, space)
    AsciiWhitespace,
    /// Other ASCII control characters (0x01-0x1F except whitespace, and 0x7F)
    AsciiOther,
    /// Non-ASCII bytes (0x80-0xFF)
    NonAscii,
}

impl ByteCategory {
    /// Categorise a byte value.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => ByteCategory::Null,
            0x09 | 0x0A | 0x0D | 0x20 => ByteCategory::AsciiWhitespace,
            0x21..=0x7E => ByteCategory::AsciiPrintable,
            0x01..=0x1F | 0x7F => ByteCategory::AsciiOther,
            0x80..=0xFF => ByteCategory::NonAscii,
        }
    }
}

/// Get a colour for a byte category derived from the syntax theme.
///
/// Maps byte categories to scope name prefixes to pull colours from the theme:
/// - Null: comment (muted)
/// - ASCII printable: string (typically green/warm)
/// - ASCII whitespace: keyword (typically blue/purple)
/// - ASCII other: punctuation/operator
/// - Non-ASCII: constant.numeric (typically orange/cyan)
fn category_colour(category: ByteCategory, theme: &syntect::highlighting::Theme) -> Colour {
    // scope prefixes to try for each category
    let scope_prefixes: &[&str] = match category {
        ByteCategory::Null => &["comment"],
        ByteCategory::AsciiPrintable => &["string", "entity.name"],
        ByteCategory::AsciiWhitespace => &["keyword", "storage"],
        ByteCategory::AsciiOther => &["punctuation", "variable"],
        ByteCategory::NonAscii => &["constant"],
    };

    let default_fg = theme
        .settings
        .foreground
        .unwrap_or(syntect::highlighting::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });

    // try each scope prefix until we find a match in the theme
    for prefix in scope_prefixes {
        for item in &theme.scopes {
            // check if any of the scope selectors start with our prefix
            let scope_str = format!("{:?}", item.scope);
            if scope_str.contains(prefix) {
                if let Some(fg) = item.style.foreground {
                    return Colour::new_rgb_bytes(fg.r, fg.g, fg.b);
                }
            }
        }
    }

    // fallback to default foreground or black
    Colour::new_rgb_bytes(default_fg.r, default_fg.g, default_fg.b)
}

/// Render binary file contents as a hex dump.
///
/// Each byte is rendered as a two-character hex pair with colour based on its category.
/// The layout engine handles line wrapping automatically, filling each line to the
/// available page width. Files exceeding `binary_hex_max_bytes` are truncated with
/// a notice indicating the limit.
///
/// Returns the page index of the first page, or None if the data was empty or the
/// hex font size is too large to fit even a single byte on the page.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    path: &Path,
    data: &[u8],
    truncated: bool,
    theme: &syntect::highlighting::Theme,
) -> Option<usize> {
    if data.is_empty() && !truncated {
        return None;
    }

    let hex_size = Pt(config.font_size_hex_pt);
    let subheading_size = Pt(config.font_size_subheading_pt);
    let text_size = Pt(config.font_size_body_pt);

    // sanity check: ensure at least one byte (2 hex chars) fits per line
    let byte_width = layout::width_of_text("00", &doc.fonts[font_ids.regular], hex_size);
    let content_width = PAGE_SIZE.0 - In(0.5).into() - In(0.25).into(); // margins
    if byte_width > content_width {
        return None;
    }

    // build hex spans with colours - let layout handle line wrapping
    let mut text: Vec<(String, Colour, SpanFont)> = Vec::new();

    for byte in data {
        let category = ByteCategory::from_byte(*byte);
        let colour = category_colour(category, theme);
        text.push((
            format!("{:02x}", byte),
            colour,
            SpanFont {
                id: font_ids.regular,
                size: hex_size,
            },
        ));
    }

    // add truncation notice if needed (on its own line)
    if truncated {
        let max_kb = config.binary_hex_max_bytes.unwrap_or(65536) / 1024;
        // two newlines: one to end the hex line, one for spacing
        text.push((
            "\n\n".to_string(),
            colours::BLACK,
            SpanFont {
                id: font_ids.regular,
                size: hex_size,
            },
        ));
        text.push((
            format!("<truncated at {}KB>", max_kb),
            Colour::new_grey(0.5),
            SpanFont {
                id: font_ids.italic,
                size: text_size,
            },
        ));
    }

    // render pages
    let mut first_page = None;
    while !text.is_empty() {
        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.page_order.len());
        let page_size = PAGE_SIZE;

        let mut page = Page::new(page_size, Some(margins));
        let start = layout::baseline_start(&page, &doc.fonts[font_ids.regular], text_size);
        let start = (
            start.0,
            start.1
                - (doc.fonts[font_ids.regular].ascent(text_size)
                    - doc.fonts[font_ids.regular].descent(subheading_size))
                - In(0.125).into(),
        );
        let bbox = page.content_box;

        // skip leading newlines
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

        header::render_header(config, doc, font_ids, &mut page, path.display())
            .expect("can render header");

        // no wrap width for hex dump (no line numbers)
        layout::layout_text_natural(doc, &mut page, start, &mut text, Pt(0.0), bbox);

        let page_id = doc.add_page(page);
        if first_page.is_none() {
            first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
        }
    }

    first_page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorises_null_bytes() {
        assert_eq!(ByteCategory::from_byte(0x00), ByteCategory::Null);
    }

    #[test]
    fn categorises_ascii_printable() {
        assert_eq!(ByteCategory::from_byte(b'A'), ByteCategory::AsciiPrintable);
        assert_eq!(ByteCategory::from_byte(b'z'), ByteCategory::AsciiPrintable);
        assert_eq!(ByteCategory::from_byte(b'!'), ByteCategory::AsciiPrintable);
        assert_eq!(ByteCategory::from_byte(b'~'), ByteCategory::AsciiPrintable);
    }

    #[test]
    fn categorises_whitespace() {
        assert_eq!(
            ByteCategory::from_byte(b'\t'),
            ByteCategory::AsciiWhitespace
        );
        assert_eq!(
            ByteCategory::from_byte(b'\n'),
            ByteCategory::AsciiWhitespace
        );
        assert_eq!(
            ByteCategory::from_byte(b'\r'),
            ByteCategory::AsciiWhitespace
        );
        assert_eq!(ByteCategory::from_byte(b' '), ByteCategory::AsciiWhitespace);
    }

    #[test]
    fn categorises_ascii_other() {
        assert_eq!(ByteCategory::from_byte(0x01), ByteCategory::AsciiOther);
        assert_eq!(ByteCategory::from_byte(0x1F), ByteCategory::AsciiOther);
        assert_eq!(ByteCategory::from_byte(0x7F), ByteCategory::AsciiOther);
    }

    #[test]
    fn categorises_non_ascii() {
        assert_eq!(ByteCategory::from_byte(0x80), ByteCategory::NonAscii);
        assert_eq!(ByteCategory::from_byte(0xFF), ByteCategory::NonAscii);
    }
}
