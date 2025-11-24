use pdf_gen::Pt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Horizontal position for headers and footers.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
pub enum Position {
    /// Alternates left/right based on page parity (for bound books)
    #[default]
    Outer,
    /// Always centred
    Centre,
    /// Alternates right/left (opposite of Outer)
    Inner,
    /// Always left-aligned
    Left,
    /// Always right-aligned
    Right,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Position::Outer => write!(f, "Outer (alternating for binding)"),
            Position::Centre => write!(f, "Centre"),
            Position::Inner => write!(f, "Inner (opposite of Outer)"),
            Position::Left => write!(f, "Left"),
            Position::Right => write!(f, "Right"),
        }
    }
}

impl Position {
    pub fn all() -> &'static [Position] {
        &[
            Position::Outer,
            Position::Centre,
            Position::Inner,
            Position::Left,
            Position::Right,
        ]
    }
}

/// Position of a horizontal rule relative to header/footer text.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
pub enum RulePosition {
    /// No rule
    #[default]
    None,
    /// Rule above the text
    Above,
    /// Rule below the text
    Below,
}

impl fmt::Display for RulePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RulePosition::None => write!(f, "None"),
            RulePosition::Above => write!(f, "Above"),
            RulePosition::Below => write!(f, "Below"),
        }
    }
}

impl RulePosition {
    pub fn all() -> &'static [RulePosition] {
        &[RulePosition::None, RulePosition::Above, RulePosition::Below]
    }
}

/// Style for page number formatting.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
pub enum PageNumberStyle {
    /// Arabic numerals (1, 2, 3)
    #[default]
    Arabic,
    /// Lowercase Roman numerals (i, ii, iii)
    RomanLower,
    /// Uppercase Roman numerals (I, II, III)
    RomanUpper,
}

impl fmt::Display for PageNumberStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageNumberStyle::Arabic => write!(f, "Arabic (1, 2, 3)"),
            PageNumberStyle::RomanLower => write!(f, "Roman lowercase (i, ii, iii)"),
            PageNumberStyle::RomanUpper => write!(f, "Roman uppercase (I, II, III)"),
        }
    }
}

/// Document section for section-specific page numbering.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Section {
    /// Front matter (README, LICENSE, etc.)
    #[default]
    Frontmatter,
    /// Source code files
    Source,
    /// Appendix content (commit history, index, etc.)
    Appendix,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Section::Frontmatter => write!(f, "Frontmatter"),
            Section::Source => write!(f, "Source"),
            Section::Appendix => write!(f, "Appendix"),
        }
    }
}

/// Page numbering configuration for a document section.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct SectionNumbering {
    /// Page number style (Arabic, Roman lowercase, Roman uppercase)
    #[serde(default)]
    pub style: PageNumberStyle,
    /// Starting page number for this section
    #[serde(default = "default_page_number_start")]
    pub start: i32,
}

impl Default for SectionNumbering {
    fn default() -> Self {
        SectionNumbering {
            style: PageNumberStyle::Arabic,
            start: 1,
        }
    }
}

impl SectionNumbering {
    /// Returns numbering config with Roman lowercase numerals starting at 1.
    pub fn roman_lower() -> Self {
        SectionNumbering {
            style: PageNumberStyle::RomanLower,
            start: 1,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum SyntaxTheme {
    #[serde(rename = "Solarized (light)")]
    SolarizedLight,
    #[serde(rename = "OneHalfLight")]
    OneHalfLight,
    #[serde(rename = "gruvbox (Light) (Hard)")]
    Gruvbox,
    #[serde(rename = "GitHub")]
    GitHub,
}

impl fmt::Display for SyntaxTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl SyntaxTheme {
    pub fn name(&self) -> &'static str {
        match self {
            SyntaxTheme::SolarizedLight => "Solarized (light)",
            SyntaxTheme::OneHalfLight => "OneHalfLight",
            SyntaxTheme::Gruvbox => "gruvbox (Light) (Hard)",
            SyntaxTheme::GitHub => "GitHub",
        }
    }

    pub fn all() -> &'static [SyntaxTheme] {
        &[
            SyntaxTheme::SolarizedLight,
            SyntaxTheme::OneHalfLight,
            SyntaxTheme::Gruvbox,
            SyntaxTheme::GitHub,
        ]
    }
}

/// Preset page sizes for the PDF output.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PageSize {
    /// Half US Letter (5.5" × 8.5")
    HalfLetter,
    /// ISO A5 (148mm × 210mm ≈ 5.83" × 8.27")
    A5,
    /// ISO A6 (105mm × 148mm ≈ 4.13" × 5.83")
    A6,
    /// Quarter US Letter (4.25" × 5.5")
    QuarterLetter,
    /// Custom dimensions
    Custom,
}

impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageSize::HalfLetter => write!(f, "Half Letter (5.5\" × 8.5\")"),
            PageSize::A5 => write!(f, "A5 (5.83\" × 8.27\")"),
            PageSize::A6 => write!(f, "A6 (4.13\" × 5.83\")"),
            PageSize::QuarterLetter => write!(f, "Quarter Letter (4.25\" × 5.5\")"),
            PageSize::Custom => write!(f, "Custom"),
        }
    }
}

impl PageSize {
    pub fn all() -> &'static [PageSize] {
        &[
            PageSize::HalfLetter,
            PageSize::A5,
            PageSize::A6,
            PageSize::QuarterLetter,
            PageSize::Custom,
        ]
    }

    /// Returns (width, height) in inches for preset sizes.
    /// Returns None for Custom (caller should use config values).
    pub fn dimensions_in(&self) -> Option<(f32, f32)> {
        match self {
            PageSize::HalfLetter => Some((5.5, 8.5)),
            PageSize::A5 => Some((5.83, 8.27)),
            PageSize::A6 => Some((4.13, 5.83)),
            PageSize::QuarterLetter => Some((4.25, 5.5)),
            PageSize::Custom => None,
        }
    }
}

/// Position of an optional image on the title page.
///
/// When an image is configured for the title page, this enum controls where
/// it appears relative to the text content. The image is always horizontally
/// centred; this setting controls vertical placement.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
pub enum TitlePageImagePosition {
    /// Image at the top, with text content below
    #[default]
    Top,
    /// Image centred on page (text flows around it, typically above)
    Centre,
    /// Image at the bottom, below text content
    Bottom,
}

impl fmt::Display for TitlePageImagePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TitlePageImagePosition::Top => write!(f, "Top"),
            TitlePageImagePosition::Centre => write!(f, "Centre"),
            TitlePageImagePosition::Bottom => write!(f, "Bottom"),
        }
    }
}

impl TitlePageImagePosition {
    pub fn all() -> &'static [TitlePageImagePosition] {
        &[
            TitlePageImagePosition::Top,
            TitlePageImagePosition::Centre,
            TitlePageImagePosition::Bottom,
        ]
    }
}

/// PDF output configuration.
///
/// Margins are asymmetric to support booklet printing: inner margins accommodate
/// binding, while outer margins can be smaller. Top margins are typically larger
/// than bottom to leave room for headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub struct PDF {
    /// Font family name ("SourceCodePro", "FiraMono") or path to custom font
    pub font: String,
    /// Syntax highlighting theme for code blocks
    pub theme: SyntaxTheme,
    /// Output PDF file path
    pub outfile: PathBuf,
    /// Page width in inches
    pub page_width_in: f32,
    /// Page height in inches
    pub page_height_in: f32,
    /// Top margin in inches (typically larger for headers)
    pub margin_top_in: f32,
    /// Outer margin in inches (away from binding)
    pub margin_outer_in: f32,
    /// Bottom margin in inches
    pub margin_bottom_in: f32,
    /// Inner margin in inches (binding/gutter side)
    pub margin_inner_in: f32,
    #[serde(default = "default_font_size_title")]
    pub font_size_title_pt: f32,
    #[serde(default = "default_font_size_heading")]
    pub font_size_heading_pt: f32,
    #[serde(default = "default_font_size_subheading")]
    pub font_size_subheading_pt: f32,
    #[serde(default = "default_font_size_body")]
    pub font_size_body_pt: f32,
    #[serde(default = "default_font_size_small")]
    pub font_size_small_pt: f32,
    /// Output path for the print-ready booklet PDF (if None, booklet not generated)
    #[serde(default)]
    pub booklet_outfile: Option<PathBuf>,
    /// Number of pages per signature (must be divisible by 4). Default is 16.
    #[serde(default = "default_booklet_signature_size")]
    pub booklet_signature_size: u32,
    /// Physical sheet width in inches for booklet printing (default 11.0 for US Letter landscape)
    #[serde(default = "default_booklet_sheet_width")]
    pub booklet_sheet_width_in: f32,
    /// Physical sheet height in inches for booklet printing (default 8.5 for US Letter landscape)
    #[serde(default = "default_booklet_sheet_height")]
    pub booklet_sheet_height_in: f32,
    /// Render binary files as coloured hex dumps instead of placeholders (default false).
    /// Warning: This dramatically increases PDF size and rendering time.
    #[serde(default)]
    pub render_binary_hex: bool,
    /// Maximum bytes per binary file before truncating (default 64KB, None for unlimited).
    /// Limits PDF bloat from large binaries while still showing representative content.
    #[serde(default = "default_binary_hex_max_bytes")]
    pub binary_hex_max_bytes: Option<usize>,
    /// Font size for hex dump text in points (default 5.0).
    /// Smaller than body text to fit more content; 5pt is near the legibility limit.
    #[serde(default = "default_font_size_hex")]
    pub font_size_hex_pt: f32,

    // Header/footer configuration
    /// Header template with placeholders: {file}, {title}, {n}, {total}
    /// Empty string disables the header.
    #[serde(default = "default_header_template")]
    pub header_template: String,
    /// Header horizontal position
    #[serde(default)]
    pub header_position: Position,
    /// Horizontal rule position relative to header
    #[serde(default = "default_header_rule")]
    pub header_rule: RulePosition,

    /// Footer template with placeholders: {file}, {title}, {n}, {total}
    /// Empty string disables the footer.
    #[serde(default = "default_footer_template")]
    pub footer_template: String,
    /// Footer horizontal position
    #[serde(default)]
    pub footer_position: Position,
    /// Horizontal rule position relative to footer
    #[serde(default)]
    pub footer_rule: RulePosition,

    /// Colophon/statistics page template with placeholders.
    /// Placeholders: {title}, {authors}, {licences}, {remotes}, {generated_date},
    /// {tool_version}, {file_count}, {line_count}, {total_bytes}, {language_stats},
    /// {commit_count}, {date_range}, {commit_chart}
    /// Empty string disables the colophon page.
    #[serde(default = "default_colophon_template")]
    pub colophon_template: String,

    // Title page configuration
    // The title page supports customisable layout via a template system.
    // Placeholders are replaced with values; fenced blocks render as monospace.

    /// Title page template with placeholders: {title}, {authors}, {licences}, {date}.
    /// The {title} placeholder is rendered in the title font; other text uses body font.
    /// Use markdown-style fenced blocks (```) for monospace text like ASCII art.
    #[serde(default = "default_title_page_template")]
    pub title_page_template: String,
    /// Optional image path for title page (logo, cover art). Supports PNG, JPG, SVG.
    /// The image is scaled to fit within max dimensions while preserving aspect ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_page_image: Option<PathBuf>,
    /// Vertical position of the title page image relative to text content.
    #[serde(default)]
    pub title_page_image_position: TitlePageImagePosition,
    /// Maximum height for the title page image in inches. Width is capped at 80% of page.
    #[serde(default = "default_title_page_image_max_height")]
    pub title_page_image_max_height_in: f32,

    // PDF document metadata
    // These populate the PDF document info dictionary, visible in PDF viewers
    // under "Properties" or "Document Info". Title and author are set automatically
    // from the source configuration; subject and keywords are optional extras.

    /// Subject/description for PDF document properties.
    /// Appears in PDF viewer document info alongside title and author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Keywords for PDF document properties (comma-separated recommended).
    /// Useful for document indexing and searchability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,

    // Section-specific page numbering
    /// Page numbering for frontmatter section (default: Roman lowercase, start 1)
    #[serde(default = "default_frontmatter_numbering")]
    pub frontmatter_numbering: SectionNumbering,
    /// Page numbering for source code section (default: Arabic, start 1)
    #[serde(default)]
    pub source_numbering: SectionNumbering,
    /// Page numbering for appendix section (default: Arabic, start 1)
    #[serde(default)]
    pub appendix_numbering: SectionNumbering,

    // Deprecated: Legacy global page numbering (pre-section support)
    // These are read from old configs but not written to new ones.
    // If present, they override section defaults for backwards compatibility.
    /// Deprecated: use section-specific numbering instead
    #[serde(default, skip_serializing)]
    pub(crate) page_number_style: Option<PageNumberStyle>,
    /// Deprecated: use section-specific numbering instead
    #[serde(default, skip_serializing)]
    pub(crate) page_number_start: Option<i32>,
}

fn default_font_size_title() -> f32 {
    32.0
}
fn default_font_size_heading() -> f32 {
    24.0
}
fn default_font_size_subheading() -> f32 {
    12.0
}
fn default_font_size_body() -> f32 {
    10.0
}
fn default_font_size_small() -> f32 {
    8.0
}
fn default_booklet_signature_size() -> u32 {
    16
}
fn default_booklet_sheet_width() -> f32 {
    11.0
}
fn default_booklet_sheet_height() -> f32 {
    8.5
}
fn default_binary_hex_max_bytes() -> Option<usize> {
    Some(65536)
}
fn default_font_size_hex() -> f32 {
    5.0
}
fn default_header_template() -> String {
    "{file}".to_string()
}
fn default_header_rule() -> RulePosition {
    RulePosition::Below
}
fn default_footer_template() -> String {
    "{n}".to_string()
}
fn default_page_number_start() -> i32 {
    1
}
fn default_frontmatter_numbering() -> SectionNumbering {
    SectionNumbering::roman_lower()
}
fn default_title_page_image_max_height() -> f32 {
    2.0
}
pub fn default_title_page_template() -> String {
    r#"{title}

- by -

{authors}"#
        .to_string()
}
pub fn default_colophon_template() -> String {
    r#"{title}

by {authors}

{remotes}

─────────────────────────────

Generated on {generated_date}
by src-book v{tool_version}

{licences}

─────────────────────────────

Statistics

  {file_count} source files
  {line_count} lines of code
  {total_bytes}
  {commit_count} commits ({date_range})

{language_stats}

Commit Activity

{commit_chart}"#
        .to_string()
}

impl Default for PDF {
    fn default() -> Self {
        PDF {
            font: "SourceCodePro".to_string(),
            theme: SyntaxTheme::GitHub,
            outfile: PathBuf::from("book.pdf"),
            page_width_in: 5.5,
            page_height_in: 8.5,
            margin_top_in: 0.5,
            margin_outer_in: 0.125,
            margin_bottom_in: 0.25,
            margin_inner_in: 0.25,
            font_size_title_pt: default_font_size_title(),
            font_size_heading_pt: default_font_size_heading(),
            font_size_subheading_pt: default_font_size_subheading(),
            font_size_body_pt: default_font_size_body(),
            font_size_small_pt: default_font_size_small(),
            booklet_outfile: None,
            booklet_signature_size: default_booklet_signature_size(),
            booklet_sheet_width_in: default_booklet_sheet_width(),
            booklet_sheet_height_in: default_booklet_sheet_height(),
            render_binary_hex: false,
            binary_hex_max_bytes: default_binary_hex_max_bytes(),
            font_size_hex_pt: default_font_size_hex(),
            header_template: default_header_template(),
            header_position: Position::default(),
            header_rule: default_header_rule(),
            footer_template: default_footer_template(),
            footer_position: Position::default(),
            footer_rule: RulePosition::default(),
            colophon_template: default_colophon_template(),
            title_page_template: default_title_page_template(),
            title_page_image: None,
            title_page_image_position: TitlePageImagePosition::default(),
            title_page_image_max_height_in: default_title_page_image_max_height(),
            subject: None,
            keywords: None,
            frontmatter_numbering: default_frontmatter_numbering(),
            source_numbering: SectionNumbering::default(),
            appendix_numbering: SectionNumbering::default(),
            page_number_style: None,
            page_number_start: None,
        }
    }
}

impl PDF {
    /// Returns the page size as (width, height) in points.
    pub fn page_size(&self) -> (Pt, Pt) {
        (
            Pt(self.page_width_in * 72.0),
            Pt(self.page_height_in * 72.0),
        )
    }

    /// Returns the numbering configuration for a given section.
    ///
    /// If legacy `page_number_style` or `page_number_start` fields are present,
    /// they override the section defaults for backwards compatibility.
    pub fn numbering_for_section(&self, section: Section) -> SectionNumbering {
        let base = match section {
            Section::Frontmatter => self.frontmatter_numbering,
            Section::Source => self.source_numbering,
            Section::Appendix => self.appendix_numbering,
        };

        // apply legacy overrides if present
        if self.page_number_style.is_some() || self.page_number_start.is_some() {
            SectionNumbering {
                style: self.page_number_style.unwrap_or(base.style),
                start: self.page_number_start.unwrap_or(base.start),
            }
        } else {
            base
        }
    }
}

/// Statistics from rendering a PDF, used for user feedback.
pub struct RenderStats {
    /// Number of pages in the main PDF
    pub page_count: usize,
    /// If a booklet was generated, the number of sheets needed
    pub booklet_sheets: Option<usize>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_serialize_pdf() {
        let pdf = PDF::default();
        toml::to_string(&pdf).expect("can serialize PDF to TOML");
    }
}
