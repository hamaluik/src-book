//! Title page rendering.
//!
//! Creates a customisable title page with template support for placeholders and
//! optional images. Templates can include markdown-style fenced blocks (```) for
//! monospace text like ASCII art or sample output.
//!
//! ## Template System
//!
//! The title page uses a simple template system similar to the colophon page.
//! Placeholders like `{title}` and `{authors}` are replaced with actual values.
//! The `{title}` placeholder receives special treatment: it's rendered in the
//! title font (bold, larger size) while other text uses the body font.
//!
//! ## Fenced Blocks
//!
//! Markdown-style triple backticks (```) denote monospace blocks. These are
//! rendered in the regular monospace font at body size, preserving spacing for
//! ASCII art. Content inside fences is not processed for placeholders.
//!
//! ## Image Support
//!
//! An optional image (logo, cover art) can be positioned at the top, centre,
//! or bottom of the page. Images are scaled to fit within configurable maximum
//! dimensions while preserving aspect ratio. Image paths are tracked for booklet
//! rendering, which needs to reload images into a separate PDF document.
//!
//! ## Layout Algorithm
//!
//! 1. Calculate total content height (text + optional image)
//! 2. Vertically centre the entire block on the page
//! 3. Render image and text segments from top to bottom
//! 4. Each text line is horizontally centred

use crate::sinks::pdf::config::{TitlePageImagePosition, PDF};
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::ImagePathMap;
use crate::source::Source;
use anyhow::Result;
use pdf_gen::*;

/// A segment of the title page template.
#[derive(Debug, Clone, PartialEq)]
enum TemplateSegment {
    /// Normal text line (may be empty for blank lines)
    Text(String),
    /// Monospace text block (contents of a fenced block)
    Mono(Vec<String>),
}

/// Expand placeholders in the title page template.
///
/// Supported placeholders:
/// - `{title}` - Book title (or "untitled" if not set)
/// - `{authors}` - Newline-separated list of authors, sorted by prominence
/// - `{licences}` - Comma-separated licence identifiers
/// - `{date}` - Current date in YYYY-MM-DD format
fn expand_template(template: &str, source: &Source) -> String {
    let title = source.title.clone().unwrap_or_else(|| "untitled".to_string());

    let mut authors = source.authors.clone();
    authors.sort();
    let authors_str = authors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    let licences = if source.licenses.is_empty() {
        "No licence specified".to_string()
    } else {
        source.licenses.join(", ")
    };

    let date = chrono::Local::now().format("%Y-%m-%d").to_string();

    template
        .replace("{title}", &title)
        .replace("{authors}", &authors_str)
        .replace("{licences}", &licences)
        .replace("{date}", &date)
}

/// Parse template content into segments, separating fenced code blocks.
///
/// Lines between ``` markers become `Mono` segments; all other lines become
/// `Text` segments. Unclosed fences are tolerated (remaining lines become a
/// mono block). The fence markers themselves are discarded.
fn parse_segments(content: &str) -> Vec<TemplateSegment> {
    let mut segments = Vec::new();
    let mut in_fence = false;
    let mut mono_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                // closing fence
                segments.push(TemplateSegment::Mono(mono_lines.clone()));
                mono_lines.clear();
                in_fence = false;
            } else {
                // opening fence
                in_fence = true;
            }
        } else if in_fence {
            mono_lines.push(line.to_string());
        } else {
            segments.push(TemplateSegment::Text(line.to_string()));
        }
    }

    // handle unclosed fence
    if in_fence && !mono_lines.is_empty() {
        segments.push(TemplateSegment::Mono(mono_lines));
    }

    segments
}

/// Calculate the height of a segment.
fn segment_height(
    segment: &TemplateSegment,
    doc: &Document,
    font_ids: &FontIds,
    title_size: Pt,
    body_size: Pt,
) -> Pt {
    match segment {
        TemplateSegment::Text(line) => {
            // title placeholder uses title font size
            if line.contains("{title}") || line.trim() == source_title_marker() {
                doc.fonts[font_ids.bold].line_height(title_size)
            } else {
                doc.fonts[font_ids.regular].line_height(body_size)
            }
        }
        TemplateSegment::Mono(lines) => {
            let line_height = doc.fonts[font_ids.regular].line_height(body_size);
            line_height * lines.len() as f32
        }
    }
}

/// Internal marker to identify the title line after placeholder expansion.
///
/// Because `{title}` is replaced with the actual title text before parsing,
/// we need a way to identify which line should use the title font. We insert
/// this marker before the title text, then skip it during rendering while
/// applying title styling to the following line.
fn source_title_marker() -> &'static str {
    "__TITLE_MARKER__"
}

/// Render the title page with customisable template and optional image.
///
/// The title page is always exactly one page. Content is vertically centred
/// on the page, with each line horizontally centred. If an image is configured,
/// its position (top/centre/bottom) affects the layout of surrounding text.
///
/// Image paths are recorded in `image_paths` so that booklet rendering can
/// reload them into its separate PDF document (images can't be shared between
/// documents, so we track paths rather than image data).
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    source: &Source,
    image_paths: &mut ImagePathMap,
) -> Result<()> {
    let title_size = Pt(config.font_size_title_pt);
    let body_size = Pt(config.font_size_body_pt);
    const SPACING: Pt = Pt(72.0 * 0.25); // spacing between image and text

    let page_size = config.page_size();
    let mut page = Page::new(page_size, None);

    // load image if configured
    let image_data = if let Some(ref image_path) = config.title_page_image {
        let image = Image::new_from_disk(image_path)?;
        let aspect_ratio = image.aspect_ratio();
        let image_id = doc.add_image(image);
        let image_index = image_id.index();

        // track for booklet rendering
        image_paths.insert(image_index, image_path.clone());

        // calculate image size (constrain by max height and page width)
        let max_height = config.title_page_image_max_height_in * 72.0;
        let max_width = page_size.0 .0 * 0.8; // 80% of page width

        let (width, height) = if aspect_ratio >= 1.0 {
            // landscape: constrain by width first
            let w = max_width.min(max_height * aspect_ratio);
            let h = w / aspect_ratio;
            (Pt(w), Pt(h.min(max_height)))
        } else {
            // portrait: constrain by height first
            let h = max_height;
            let w = (h * aspect_ratio).min(max_width);
            (Pt(w), Pt(h))
        };

        Some((image_index, width, height))
    } else {
        None
    };

    // expand template and parse into segments
    // temporarily mark title for identification after expansion
    let template = config.title_page_template.replace("{title}", &format!("{}\n{}", source_title_marker(), source.title.clone().unwrap_or_else(|| "untitled".to_string())));
    let content = expand_template(&template, source);
    let segments = parse_segments(&content);

    // calculate total content height
    let image_height = image_data.map(|(_, _, h)| h + SPACING).unwrap_or(Pt(0.0));
    let text_height: Pt = segments
        .iter()
        .map(|s| segment_height(s, doc, font_ids, title_size, body_size))
        .sum();
    let total_height = image_height + text_height;

    // determine starting y position based on image position
    let (image_y, text_start_y) = match config.title_page_image_position {
        TitlePageImagePosition::Top => {
            let start_y = (page_size.1 + total_height) / 2.0;
            let image_y = image_data.as_ref().map(|(_, _, h)| start_y - *h);
            let text_y = start_y - image_height;
            (image_y, text_y)
        }
        TitlePageImagePosition::Centre => {
            // image in centre, text above and below (text flows around)
            // for simplicity, put image in centre of page, text above it
            let image_y = image_data.as_ref().map(|(_, _, h)| (page_size.1 + *h) / 2.0 - *h);
            let text_y = (page_size.1 + total_height) / 2.0;
            (image_y, text_y)
        }
        TitlePageImagePosition::Bottom => {
            let start_y = (page_size.1 + total_height) / 2.0;
            let text_y = start_y;
            let image_y = image_data.as_ref().map(|(_, _, h)| start_y - text_height - SPACING - *h + *h);
            (image_y, text_y)
        }
    };

    // render image if present and position is Top
    if let (Some((image_index, width, height)), Some(img_y)) = (&image_data, image_y) {
        if config.title_page_image_position == TitlePageImagePosition::Top {
            let x = (page_size.0 - *width) / 2.0;
            page.add_image(ImageLayout {
                image_index: *image_index,
                position: Rect {
                    x1: x,
                    y1: img_y - *height,
                    x2: x + *width,
                    y2: img_y,
                },
            });
        }
    }

    // render text segments
    let mut y = text_start_y;
    for segment in &segments {
        match segment {
            TemplateSegment::Text(line) => {
                let is_title = line.trim() == source_title_marker();
                if is_title {
                    // skip the marker line, actual title follows
                    continue;
                }

                let (font_id, size) = if segments.iter().any(|s| matches!(s, TemplateSegment::Text(t) if t.trim() == source_title_marker()))
                    && segments.iter().position(|s| matches!(s, TemplateSegment::Text(t) if t.trim() == source_title_marker())).map(|i| segments.get(i + 1)).flatten() == Some(segment) {
                    // this is the line after the title marker
                    (font_ids.bold, title_size)
                } else {
                    (font_ids.regular, body_size)
                };

                let line_height = doc.fonts[font_id].line_height(size);
                let text_width = layout::width_of_text(line, &doc.fonts[font_id], size);
                let x = (page_size.0 - text_width) / 2.0;

                if !line.is_empty() {
                    page.add_span(SpanLayout {
                        text: line.clone(),
                        font: SpanFont { id: font_id, size },
                        colour: colours::BLACK,
                        coords: (x, y),
                    });
                }
                y -= line_height;
            }
            TemplateSegment::Mono(lines) => {
                let line_height = doc.fonts[font_ids.regular].line_height(body_size);
                for line in lines {
                    let text_width = layout::width_of_text(line, &doc.fonts[font_ids.regular], body_size);
                    let x = (page_size.0 - text_width) / 2.0;
                    page.add_span(SpanLayout {
                        text: line.clone(),
                        font: SpanFont {
                            id: font_ids.regular,
                            size: body_size,
                        },
                        colour: colours::BLACK,
                        coords: (x, y),
                    });
                    y -= line_height;
                }
            }
        }
    }

    // render image if position is Centre or Bottom
    if let Some((image_index, width, height)) = &image_data {
        let render_now = match config.title_page_image_position {
            TitlePageImagePosition::Top => false,
            TitlePageImagePosition::Centre => true,
            TitlePageImagePosition::Bottom => true,
        };
        if render_now {
            let x = (page_size.0 - *width) / 2.0;
            let image_y = match config.title_page_image_position {
                TitlePageImagePosition::Centre => (page_size.1 - *height) / 2.0,
                TitlePageImagePosition::Bottom => y - SPACING,
                TitlePageImagePosition::Top => unreachable!(),
            };
            page.add_image(ImageLayout {
                image_index: *image_index,
                position: Rect {
                    x1: x,
                    y1: image_y,
                    x2: x + *width,
                    y2: image_y + *height,
                },
            });
        }
    }

    doc.add_page(page);
    Ok(())
}
