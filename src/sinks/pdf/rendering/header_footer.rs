//! Header and footer rendering with customisable templates.
//!
//! Renders headers and footers on content pages using user-defined templates.
//! Templates support placeholders:
//! - `{file}` - current file path
//! - `{title}` - book title
//! - `{n}` - page number (formatted per page_number_style)
//! - `{total}` - total page count
//!
//! Position can be Outer (alternating for binding), Centre, Inner, Left, or Right.
//! Optional horizontal rules can be placed Above or Below the text.

use crate::sinks::pdf::config::{PageNumberStyle, Position, RulePosition, Section, PDF};
use crate::sinks::pdf::fonts::FontIds;
use owned_ttf_parser::AsFaceRef;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;

/// Metadata tracked for each page during rendering.
#[derive(Clone, Debug, Default)]
pub struct PageMetadata {
    /// File path displayed on this page (if any)
    pub file_path: Option<String>,
    /// Which section this page belongs to
    pub section: Section,
    /// Page index within the section (0-indexed)
    pub page_in_section: usize,
}

impl PageMetadata {
    pub fn new(section: Section, page_in_section: usize) -> Self {
        Self {
            file_path: None,
            section,
            page_in_section,
        }
    }

    pub fn with_file(mut self, file_path: impl Into<String>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }
}

/// Tracks total page counts per section for `{total}` placeholder.
#[derive(Clone, Debug, Default)]
pub struct SectionTotals {
    pub frontmatter: usize,
    pub source: usize,
    pub appendix: usize,
}

impl SectionTotals {
    /// Returns the total pages for the given section.
    pub fn total_for(&self, section: Section) -> usize {
        match section {
            Section::Frontmatter => self.frontmatter,
            Section::Source => self.source,
            Section::Appendix => self.appendix,
        }
    }
}

/// Convert a number to Roman numerals.
fn to_roman(mut n: i32) -> String {
    if n <= 0 {
        // handle zero/negative by returning arabic
        return n.to_string();
    }

    let numerals = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];

    let mut result = String::new();
    for (value, numeral) in numerals {
        while n >= value {
            result.push_str(numeral);
            n -= value;
        }
    }
    result
}

/// Format a page number according to the specified style.
pub fn format_page_number(n: i32, style: PageNumberStyle) -> String {
    match style {
        PageNumberStyle::Arabic => n.to_string(),
        PageNumberStyle::RomanLower => to_roman(n),
        PageNumberStyle::RomanUpper => to_roman(n).to_uppercase(),
    }
}

/// Expand a template string with placeholder values using section-aware numbering.
///
/// The page number is calculated as: section_start + page_in_section
/// The total is the section's page count, not the entire document.
fn expand_template(
    template: &str,
    file_path: Option<&str>,
    title: Option<&str>,
    metadata: &PageMetadata,
    section_totals: &SectionTotals,
    config: &PDF,
) -> String {
    let numbering = config.numbering_for_section(metadata.section);
    let page_number = numbering.start + metadata.page_in_section as i32;
    let section_total = section_totals.total_for(metadata.section);

    let page_str = format_page_number(page_number, numbering.style);
    let total_str = format_page_number(section_total as i32, numbering.style);

    template
        .replace("{file}", file_path.unwrap_or(""))
        .replace("{title}", title.unwrap_or(""))
        .replace("{n}", &page_str)
        .replace("{total}", &total_str)
}

/// Calculate the x-coordinate for text based on position and page parity.
fn calculate_x_position(
    position: Position,
    page_index: usize,
    content_box: &Rect,
    text_width: Pt,
) -> Pt {
    // even indices are right-hand (recto) pages in a bound book
    let is_recto = page_index % 2 == 0;

    match position {
        Position::Outer => {
            if is_recto {
                content_box.x2 - text_width
            } else {
                content_box.x1
            }
        }
        Position::Inner => {
            if is_recto {
                content_box.x1
            } else {
                content_box.x2 - text_width
            }
        }
        Position::Centre => {
            let content_width = content_box.x2 - content_box.x1;
            content_box.x1 + (content_width - text_width) / 2.0
        }
        Position::Left => content_box.x1,
        Position::Right => content_box.x2 - text_width,
    }
}

/// Render a horizontal rule across the content box.
fn render_rule(page: &mut Page, content_box: &Rect, y: Pt, thickness: Pt) {
    let mut content = Content::new();
    content
        .set_stroke_gray(0.75)
        .set_line_cap(LineCapStyle::ButtCap)
        .set_line_width(*thickness)
        .move_to(*content_box.x1, *y)
        .line_to(*content_box.x2, *y)
        .stroke();
    page.add_content(content);
}

/// Calculate total page counts per section from page metadata.
pub fn calculate_section_totals(page_metadata: &[PageMetadata]) -> SectionTotals {
    let mut totals = SectionTotals::default();
    for meta in page_metadata {
        match meta.section {
            Section::Frontmatter => totals.frontmatter += 1,
            Section::Source => totals.source += 1,
            Section::Appendix => totals.appendix += 1,
        }
    }
    totals
}

/// Render headers and footers on all content pages.
///
/// This should be called after all content is rendered, when the total page
/// count is known. The `page_offset` indicates where content pages start
/// (skipping title page, blank page, and TOC).
pub fn render_headers_and_footers(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    page_offset: usize,
    page_metadata: &[PageMetadata],
    title: Option<&str>,
) {
    // calculate section totals from page metadata
    let section_totals = calculate_section_totals(page_metadata);

    let header_size = Pt(config.font_size_subheading_pt);
    let footer_size = Pt(config.font_size_small_pt);

    // get underline metrics for rules
    let (line_offset, line_thickness) = doc.fonts[font_ids.regular]
        .face
        .as_face_ref()
        .underline_metrics()
        .map(|metrics| {
            let scaling = header_size
                / doc.fonts[font_ids.regular]
                    .face
                    .as_face_ref()
                    .units_per_em() as f32;
            (
                scaling * metrics.position as f32,
                scaling * metrics.thickness as f32,
            )
        })
        .unwrap_or_else(|| (Pt(-2.0), Pt(0.5)));

    for (pi, page_id) in doc.page_order.iter().skip(page_offset).enumerate() {
        let metadata = page_metadata.get(pi).cloned().unwrap_or_default();

        let page = doc.pages.get_mut(*page_id).expect("page exists");
        let content_box = page.content_box;

        // render header if template is non-empty
        if !config.header_template.is_empty() {
            let text = expand_template(
                &config.header_template,
                metadata.file_path.as_deref(),
                title,
                &metadata,
                &section_totals,
                config,
            );

            if !text.is_empty() {
                let text_width =
                    layout::width_of_text(&text, &doc.fonts[font_ids.regular], header_size);
                let x = calculate_x_position(config.header_position, pi, &content_box, text_width);

                // header at top of content box
                let y = content_box.y2 - doc.fonts[font_ids.regular].ascent(header_size);

                page.add_span(SpanLayout {
                    text,
                    font: SpanFont {
                        id: font_ids.regular,
                        size: header_size,
                    },
                    colour: Colour::new_grey(0.25),
                    coords: (x, y),
                });

                // render header rule
                let baseline_y = y;
                match config.header_rule {
                    RulePosition::None => {}
                    RulePosition::Above => {
                        let rule_y =
                            baseline_y + doc.fonts[font_ids.regular].ascent(header_size) + Pt(2.0);
                        render_rule(page, &content_box, rule_y, line_thickness);
                    }
                    RulePosition::Below => {
                        let rule_y = baseline_y + line_offset;
                        render_rule(page, &content_box, rule_y, line_thickness);
                    }
                }
            }
        }

        // render footer if template is non-empty
        if !config.footer_template.is_empty() {
            let text = expand_template(
                &config.footer_template,
                metadata.file_path.as_deref(),
                title,
                &metadata,
                &section_totals,
                config,
            );

            if !text.is_empty() {
                let text_width =
                    layout::width_of_text(&text, &doc.fonts[font_ids.regular], footer_size);
                let x = calculate_x_position(config.footer_position, pi, &content_box, text_width);

                // footer near bottom of page
                let y: Pt = In(0.25).into();

                page.add_span(SpanLayout {
                    text,
                    font: SpanFont {
                        id: font_ids.regular,
                        size: footer_size,
                    },
                    colour: Colour::new_grey(0.25),
                    coords: (x, y),
                });

                // render footer rule
                match config.footer_rule {
                    RulePosition::None => {}
                    RulePosition::Above => {
                        let rule_y = y + doc.fonts[font_ids.regular].ascent(footer_size) + Pt(2.0);
                        render_rule(page, &content_box, rule_y, line_thickness);
                    }
                    RulePosition::Below => {
                        let rule_y = y + line_offset;
                        render_rule(page, &content_box, rule_y, line_thickness);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_convert_to_roman_numerals() {
        assert_eq!(to_roman(1), "i");
        assert_eq!(to_roman(4), "iv");
        assert_eq!(to_roman(9), "ix");
        assert_eq!(to_roman(14), "xiv");
        assert_eq!(to_roman(42), "xlii");
        assert_eq!(to_roman(99), "xcix");
        assert_eq!(to_roman(100), "c");
        assert_eq!(to_roman(399), "cccxcix");
        assert_eq!(to_roman(500), "d");
        assert_eq!(to_roman(1984), "mcmlxxxiv");
    }

    #[test]
    fn can_format_page_numbers() {
        assert_eq!(format_page_number(42, PageNumberStyle::Arabic), "42");
        assert_eq!(format_page_number(42, PageNumberStyle::RomanLower), "xlii");
        assert_eq!(format_page_number(42, PageNumberStyle::RomanUpper), "XLII");
    }

    #[test]
    fn can_expand_template() {
        let config = PDF::default();
        let metadata = PageMetadata::new(Section::Source, 4).with_file("src/main.rs");
        let totals = SectionTotals {
            frontmatter: 0,
            source: 100,
            appendix: 0,
        };
        let result = expand_template(
            "Page {n} of {total} - {file}",
            metadata.file_path.as_deref(),
            Some("My Book"),
            &metadata,
            &totals,
            &config,
        );
        // source numbering defaults to Arabic starting at 1, so page_in_section=4 → page 5
        assert_eq!(result, "Page 5 of 100 - src/main.rs");
    }

    #[test]
    fn can_expand_template_with_roman() {
        let config = PDF::default();
        // frontmatter numbering defaults to Roman lowercase starting at 1
        let metadata = PageMetadata::new(Section::Frontmatter, 3);
        let totals = SectionTotals {
            frontmatter: 10,
            source: 0,
            appendix: 0,
        };
        let result = expand_template(
            "- {n} -",
            metadata.file_path.as_deref(),
            None,
            &metadata,
            &totals,
            &config,
        );
        // frontmatter page_in_section=3 + start=1 → page 4 in Roman = iv
        assert_eq!(result, "- iv -");
    }

    #[test]
    fn can_calculate_section_totals() {
        let metadata = vec![
            PageMetadata::new(Section::Frontmatter, 0),
            PageMetadata::new(Section::Frontmatter, 1),
            PageMetadata::new(Section::Source, 0),
            PageMetadata::new(Section::Source, 1),
            PageMetadata::new(Section::Source, 2),
            PageMetadata::new(Section::Appendix, 0),
        ];
        let totals = calculate_section_totals(&metadata);
        assert_eq!(totals.frontmatter, 2);
        assert_eq!(totals.source, 3);
        assert_eq!(totals.appendix, 1);
    }
}
