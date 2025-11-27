//! Source file rendering with syntax highlighting.
//!
//! Renders source code files with line numbers, syntax highlighting based on file
//! extension, and natural text wrapping. Binary files can be rendered as hex dumps
//! (when enabled) or display a placeholder.
//!
//! ## Parallelisation-Ready Design
//!
//! The [`render()`] function returns a [`RenderResult`] containing `Vec<Page>` rather
//! than adding pages directly to the document. This design enables parallel rendering:
//! multiple files can be rendered concurrently (each producing independent pages), then
//! their pages can be added to the document sequentially in the correct order.
//!
//! The function only requires immutable access to the document (`&Document`) for font
//! metrics, making it safe to call from multiple threads with rayon's `par_iter()`.

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
///
/// Contains the rendered pages and metadata needed to create bookmarks.
/// Pages are returned rather than added directly to the document, enabling
/// parallel rendering of multiple files followed by sequential insertion.
pub struct RenderResult {
    /// Pages rendered for this file. May be empty for files with no content.
    pub pages: Vec<Page>,
    /// Title to use for the bookmark (typically the filename).
    pub bookmark_title: String,
}

/// Render a source file with syntax highlighting.
///
/// Text files are rendered with line numbers and syntax highlighting based on file
/// extension. Binary files (detected by UTF-8 decode failure) are either rendered
/// as hex dumps (when `config.binary_hex.enabled` is enabled) or shown as a grey
/// placeholder.
///
/// Returns a [`RenderResult`] containing the rendered pages and bookmark title.
/// Pages are returned rather than added to the document directly, enabling parallel
/// rendering when used with rayon.
pub fn render(
    config: &PDF,
    doc: &Document,
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

                return hex_dump::render(config, doc, font_ids, path, data, truncated, theme);
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
    let mut pages = Vec::new();
    let mut page_index = 0;
    while !text.is_empty() {
        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), page_index);
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
        pages.push(page);
        page_index += 1;
    }

    let bookmark_title = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    Ok(RenderResult {
        pages,
        bookmark_title,
    })
}
