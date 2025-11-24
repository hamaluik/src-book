//! Source file rendering with syntax highlighting.
//!
//! Renders source code files with line numbers, syntax highlighting based on file
//! extension, and natural text wrapping. Binary files can be rendered as hex dumps
//! (when enabled) or display a placeholder.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::hex_dump;
use anyhow::{Context, Result};
use pdf_gen::layout::Margins;
use pdf_gen::*;
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::FontStyle;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Result of rendering a source file.
pub struct RenderResult {
    /// Page index of the first page, or None if the file was empty
    pub first_page: Option<usize>,
    /// Number of pages rendered
    pub page_count: usize,
}

/// Render a source file with syntax highlighting.
///
/// Text files are rendered with line numbers and syntax highlighting based on file
/// extension. Binary files (detected by UTF-8 decode failure) are either rendered
/// as hex dumps (when `config.binary_hex.enabled` is enabled) or shown as a grey
/// placeholder.
///
/// Returns the first page index and number of pages rendered.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    path: &Path,
    ss: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
) -> Result<RenderResult> {
    let text_size = Pt(config.fonts.body_pt);
    let small_size = Pt(config.fonts.small_pt);
    let subheading_size = Pt(config.fonts.subheading_pt);

    // read the contents, or handle binary files
    let (contents, is_binary) = match std::fs::read_to_string(path) {
        Ok(contents) => (contents.replace("    ", "  "), false),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            // binary file - check if we should render as hex
            if config.binary_hex.enabled {
                let data = std::fs::read(path)
                    .with_context(|| format!("Failed to read binary file {}", path.display()))?;

                let max_bytes = config.binary_hex.max_bytes.unwrap_or(usize::MAX);
                let truncated = data.len() > max_bytes;
                let data = if truncated {
                    &data[..max_bytes]
                } else {
                    &data[..]
                };

                return Ok(hex_dump::render(
                    config, doc, font_ids, path, data, truncated, theme,
                ));
            }
            // fallback to placeholder
            ("<binary data>".to_string(), true)
        }
        Err(e) => {
            return Err(e)
                .with_context(|| format!("Failed to read contents of {}", path.display()));
        }
    };

    // figure out the syntax if we can (skip for binary files)
    let syntax = if is_binary {
        None
    } else {
        ss.find_syntax_by_extension(
            path.extension()
                .map(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .unwrap_or_default(),
        )
    };

    // start the set of pages with the path
    let mut text: Vec<(String, Colour, SpanFont)> = Vec::default();

    if is_binary {
        // render binary placeholder
        text.push((
            contents,
            Colour::new_grey(0.5),
            SpanFont {
                id: font_ids.italic,
                size: text_size,
            },
        ));
    } else if let Some(syntax) = syntax {
        // load the contents of the file
        let mut h = HighlightLines::new(syntax, theme);

        // highlight the file, converting into spans
        for (i, line) in LinesWithEndings::from(contents.as_str()).enumerate() {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = h
                .highlight_line(line, ss)
                .with_context(|| format!("Failed to highlight source code for line `{}`", line))?;

            text.push((
                format!("{:>4}  ", i + 1),
                Colour::new_grey(0.75),
                SpanFont {
                    id: font_ids.regular,
                    size: small_size,
                },
            ));
            for (style, s) in ranges.into_iter() {
                let colour = Colour::new_rgb_bytes(
                    style.foreground.r,
                    style.foreground.g,
                    style.foreground.b,
                );

                let font_id = match (
                    style.font_style.intersects(FontStyle::BOLD),
                    style.font_style.intersects(FontStyle::ITALIC),
                ) {
                    (true, true) => font_ids.bold_italic,
                    (true, false) => font_ids.bold,
                    (false, true) => font_ids.italic,
                    (false, false) => font_ids.regular,
                };

                text.push((
                    s.to_string(),
                    colour,
                    SpanFont {
                        id: font_id,
                        size: text_size,
                    },
                ));
            }
        }
    } else {
        // render without syntax highlighting
        // note: don't show line numbers on these files
        for line in contents.lines() {
            text.push((
                format!("{}\n", line),
                colours::BLACK,
                SpanFont {
                    id: font_ids.regular,
                    size: text_size,
                },
            ));
        }
    }

    // and render it into pages
    let wrap_width = if syntax.is_some() {
        layout::width_of_text("      ", &doc.fonts[font_ids.regular], small_size)
    } else {
        Pt(0.0)
    };
    let mut first_page = None;
    let mut page_count = 0;
    while !text.is_empty() {
        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.page_order.len());
        let page_size = config.page_size();

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
        page_count += 1;
        if first_page.is_none() {
            first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
        }
    }

    Ok(RenderResult {
        first_page,
        page_count,
    })
}
