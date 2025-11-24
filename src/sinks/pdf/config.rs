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

// ─────────────────────────────────────────────────────────────────────────────
// Nested configuration structs
// ─────────────────────────────────────────────────────────────────────────────

/// Page dimensions configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    /// Page width in inches
    pub width_in: f32,
    /// Page height in inches
    pub height_in: f32,
}

impl Default for PageConfig {
    fn default() -> Self {
        Self {
            width_in: 5.5,
            height_in: 8.5,
        }
    }
}

/// Page margin configuration.
///
/// Margins are asymmetric to support booklet printing: inner margins accommodate
/// binding, while outer margins can be smaller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginsConfig {
    /// Top margin in inches (typically larger for headers)
    pub top_in: f32,
    /// Bottom margin in inches
    pub bottom_in: f32,
    /// Inner margin in inches (binding/gutter side)
    pub inner_in: f32,
    /// Outer margin in inches (away from binding)
    pub outer_in: f32,
}

impl Default for MarginsConfig {
    fn default() -> Self {
        Self {
            top_in: 0.5,
            bottom_in: 0.25,
            inner_in: 0.25,
            outer_in: 0.125,
        }
    }
}

/// Font size configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontSizesConfig {
    /// Title font size in points
    pub title_pt: f32,
    /// Heading font size in points
    pub heading_pt: f32,
    /// Subheading font size in points
    pub subheading_pt: f32,
    /// Body text font size in points
    pub body_pt: f32,
    /// Small text font size in points
    pub small_pt: f32,
}

impl Default for FontSizesConfig {
    fn default() -> Self {
        Self {
            title_pt: 32.0,
            heading_pt: 24.0,
            subheading_pt: 12.0,
            body_pt: 10.0,
            small_pt: 8.0,
        }
    }
}

/// Header configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderConfig {
    /// Template with placeholders: {file}, {title}, {n}, {total}.
    /// Empty string disables the header.
    pub template: String,
    /// Horizontal position
    pub position: Position,
    /// Horizontal rule position relative to text
    pub rule: RulePosition,
}

impl Default for HeaderConfig {
    fn default() -> Self {
        Self {
            template: "{file}".to_string(),
            position: Position::Outer,
            rule: RulePosition::Below,
        }
    }
}

/// Footer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FooterConfig {
    /// Template with placeholders: {file}, {title}, {n}, {total}.
    /// Empty string disables the footer.
    pub template: String,
    /// Horizontal position
    pub position: Position,
    /// Horizontal rule position relative to text
    pub rule: RulePosition,
}

impl Default for FooterConfig {
    fn default() -> Self {
        Self {
            template: "{n}".to_string(),
            position: Position::Outer,
            rule: RulePosition::None,
        }
    }
}

/// Title page configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitlePageConfig {
    /// Template with placeholders: {title}, {authors}, {licences}, {date}.
    /// Use markdown-style fenced blocks (```) for monospace text like ASCII art.
    pub template: String,
    /// Optional image path (logo, cover art). Empty string for none.
    pub image: String,
    /// Vertical position of the image relative to text content
    pub image_position: TitlePageImagePosition,
    /// Maximum height for the image in inches
    pub image_max_height_in: f32,
}

impl Default for TitlePageConfig {
    fn default() -> Self {
        Self {
            template: default_title_page_template(),
            image: String::new(),
            image_position: TitlePageImagePosition::Top,
            image_max_height_in: 2.0,
        }
    }
}

/// Colophon/statistics page configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColophonConfig {
    /// Template with placeholders. Empty string disables the colophon page.
    /// Placeholders: {title}, {authors}, {licences}, {remotes}, {generated_date},
    /// {tool_version}, {file_count}, {line_count}, {total_bytes}, {language_stats},
    /// {commit_count}, {date_range}, {commit_chart}
    pub template: String,
}

impl Default for ColophonConfig {
    fn default() -> Self {
        Self {
            template: default_colophon_template(),
        }
    }
}

/// PDF document metadata configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataConfig {
    /// Subject/description for PDF document properties.
    /// Empty string for none.
    pub subject: String,
    /// Keywords for PDF document properties (comma-separated recommended).
    /// Empty string for none.
    pub keywords: String,
}

/// Booklet printing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookletConfig {
    /// Output path for print-ready booklet PDF. Empty string disables booklet generation.
    pub outfile: String,
    /// Number of pages per signature (must be divisible by 4)
    pub signature_size: u32,
    /// Physical sheet width in inches (default 11.0 for US Letter landscape)
    pub sheet_width_in: f32,
    /// Physical sheet height in inches (default 8.5 for US Letter landscape)
    pub sheet_height_in: f32,
}

impl Default for BookletConfig {
    fn default() -> Self {
        Self {
            outfile: String::new(),
            signature_size: 16,
            sheet_width_in: 11.0,
            sheet_height_in: 8.5,
        }
    }
}

/// Binary file hex dump rendering configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryHexConfig {
    /// Render binary files as coloured hex dumps instead of placeholders.
    /// Warning: This dramatically increases PDF size and rendering time.
    pub enabled: bool,
    /// Maximum bytes per binary file before truncating (None for unlimited).
    pub max_bytes: Option<usize>,
    /// Font size for hex dump text in points
    pub font_size_pt: f32,
}

impl Default for BinaryHexConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_bytes: Some(65536),
            font_size_pt: 5.0,
        }
    }
}

/// Page numbering configuration for all document sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberingConfig {
    /// Numbering for frontmatter section
    pub frontmatter: SectionNumbering,
    /// Numbering for source code section
    pub source: SectionNumbering,
    /// Numbering for appendix section
    pub appendix: SectionNumbering,
}

impl Default for NumberingConfig {
    fn default() -> Self {
        Self {
            frontmatter: SectionNumbering::roman_lower(),
            source: SectionNumbering::default(),
            appendix: SectionNumbering::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main PDF configuration struct
// ─────────────────────────────────────────────────────────────────────────────

/// PDF output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub struct PDF {
    /// Output PDF file path
    pub outfile: PathBuf,
    /// Font family name ("SourceCodePro", "FiraMono") or path to custom font
    pub font: String,
    /// Syntax highlighting theme for code blocks
    pub theme: SyntaxTheme,

    /// Page dimensions
    pub page: PageConfig,
    /// Page margins (asymmetric for binding)
    pub margins: MarginsConfig,
    /// Font sizes
    pub fonts: FontSizesConfig,

    /// Header configuration
    pub header: HeaderConfig,
    /// Footer configuration
    pub footer: FooterConfig,

    /// Title page configuration
    pub title_page: TitlePageConfig,
    /// Colophon/statistics page configuration
    pub colophon: ColophonConfig,

    /// PDF document metadata
    pub metadata: MetadataConfig,

    /// Booklet printing configuration
    pub booklet: BookletConfig,
    /// Binary file hex dump rendering
    pub binary_hex: BinaryHexConfig,

    /// Section-specific page numbering
    pub numbering: NumberingConfig,

    // ─────────────────────────────────────────────────────────────────────────
    // Deprecated fields for backwards compatibility
    // These are read from old configs but not written to new ones.
    // ─────────────────────────────────────────────────────────────────────────

    // Legacy flat field names (read for migration, not written)
    #[serde(default, skip_serializing)]
    pub(crate) page_width_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) page_height_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) margin_top_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) margin_outer_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) margin_bottom_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) margin_inner_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_title_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_heading_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_subheading_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_body_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_small_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) booklet_outfile: Option<PathBuf>,
    #[serde(default, skip_serializing)]
    pub(crate) booklet_signature_size: Option<u32>,
    #[serde(default, skip_serializing)]
    pub(crate) booklet_sheet_width_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) booklet_sheet_height_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) render_binary_hex: Option<bool>,
    #[serde(default, skip_serializing)]
    pub(crate) binary_hex_max_bytes: Option<Option<usize>>,
    #[serde(default, skip_serializing)]
    pub(crate) font_size_hex_pt: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) header_template: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) header_position: Option<Position>,
    #[serde(default, skip_serializing)]
    pub(crate) header_rule: Option<RulePosition>,
    #[serde(default, skip_serializing)]
    pub(crate) footer_template: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) footer_position: Option<Position>,
    #[serde(default, skip_serializing)]
    pub(crate) footer_rule: Option<RulePosition>,
    #[serde(default, skip_serializing)]
    pub(crate) colophon_template: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) title_page_template: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) title_page_image: Option<PathBuf>,
    #[serde(default, skip_serializing)]
    pub(crate) title_page_image_position: Option<TitlePageImagePosition>,
    #[serde(default, skip_serializing)]
    pub(crate) title_page_image_max_height_in: Option<f32>,
    #[serde(default, skip_serializing)]
    pub(crate) subject: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) keywords: Option<String>,
    #[serde(default, skip_serializing)]
    pub(crate) frontmatter_numbering: Option<SectionNumbering>,
    #[serde(default, skip_serializing)]
    pub(crate) source_numbering: Option<SectionNumbering>,
    #[serde(default, skip_serializing)]
    pub(crate) appendix_numbering: Option<SectionNumbering>,
    #[serde(default, skip_serializing)]
    pub(crate) page_number_style: Option<PageNumberStyle>,
    #[serde(default, skip_serializing)]
    pub(crate) page_number_start: Option<i32>,
}

fn default_page_number_start() -> i32 {
    1
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
            outfile: PathBuf::from("book.pdf"),
            font: "SourceCodePro".to_string(),
            theme: SyntaxTheme::GitHub,
            page: PageConfig::default(),
            margins: MarginsConfig::default(),
            fonts: FontSizesConfig::default(),
            header: HeaderConfig::default(),
            footer: FooterConfig::default(),
            title_page: TitlePageConfig::default(),
            colophon: ColophonConfig::default(),
            metadata: MetadataConfig::default(),
            booklet: BookletConfig::default(),
            binary_hex: BinaryHexConfig::default(),
            numbering: NumberingConfig::default(),
            // legacy fields
            page_width_in: None,
            page_height_in: None,
            margin_top_in: None,
            margin_outer_in: None,
            margin_bottom_in: None,
            margin_inner_in: None,
            font_size_title_pt: None,
            font_size_heading_pt: None,
            font_size_subheading_pt: None,
            font_size_body_pt: None,
            font_size_small_pt: None,
            booklet_outfile: None,
            booklet_signature_size: None,
            booklet_sheet_width_in: None,
            booklet_sheet_height_in: None,
            render_binary_hex: None,
            binary_hex_max_bytes: None,
            font_size_hex_pt: None,
            header_template: None,
            header_position: None,
            header_rule: None,
            footer_template: None,
            footer_position: None,
            footer_rule: None,
            colophon_template: None,
            title_page_template: None,
            title_page_image: None,
            title_page_image_position: None,
            title_page_image_max_height_in: None,
            subject: None,
            keywords: None,
            frontmatter_numbering: None,
            source_numbering: None,
            appendix_numbering: None,
            page_number_style: None,
            page_number_start: None,
        }
    }
}

impl PDF {
    /// Returns the page size as (width, height) in points.
    pub fn page_size(&self) -> (Pt, Pt) {
        (
            Pt(self.page.width_in * 72.0),
            Pt(self.page.height_in * 72.0),
        )
    }

    /// Returns the numbering configuration for a given section.
    ///
    /// If legacy `page_number_style` or `page_number_start` fields are present,
    /// they override the section defaults for backwards compatibility.
    pub fn numbering_for_section(&self, section: Section) -> SectionNumbering {
        let base = match section {
            Section::Frontmatter => self.numbering.frontmatter,
            Section::Source => self.numbering.source,
            Section::Appendix => self.numbering.appendix,
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

    /// Applies legacy flat field values to their new nested locations.
    /// Called after deserialization to migrate old config formats.
    pub fn apply_legacy_fields(&mut self) {
        // page dimensions
        if let Some(v) = self.page_width_in {
            self.page.width_in = v;
        }
        if let Some(v) = self.page_height_in {
            self.page.height_in = v;
        }

        // margins
        if let Some(v) = self.margin_top_in {
            self.margins.top_in = v;
        }
        if let Some(v) = self.margin_bottom_in {
            self.margins.bottom_in = v;
        }
        if let Some(v) = self.margin_inner_in {
            self.margins.inner_in = v;
        }
        if let Some(v) = self.margin_outer_in {
            self.margins.outer_in = v;
        }

        // font sizes
        if let Some(v) = self.font_size_title_pt {
            self.fonts.title_pt = v;
        }
        if let Some(v) = self.font_size_heading_pt {
            self.fonts.heading_pt = v;
        }
        if let Some(v) = self.font_size_subheading_pt {
            self.fonts.subheading_pt = v;
        }
        if let Some(v) = self.font_size_body_pt {
            self.fonts.body_pt = v;
        }
        if let Some(v) = self.font_size_small_pt {
            self.fonts.small_pt = v;
        }

        // booklet
        if let Some(v) = self.booklet_outfile.take() {
            self.booklet.outfile = v.to_string_lossy().to_string();
        }
        if let Some(v) = self.booklet_signature_size {
            self.booklet.signature_size = v;
        }
        if let Some(v) = self.booklet_sheet_width_in {
            self.booklet.sheet_width_in = v;
        }
        if let Some(v) = self.booklet_sheet_height_in {
            self.booklet.sheet_height_in = v;
        }

        // binary hex
        if let Some(v) = self.render_binary_hex {
            self.binary_hex.enabled = v;
        }
        if let Some(v) = self.binary_hex_max_bytes {
            self.binary_hex.max_bytes = v;
        }
        if let Some(v) = self.font_size_hex_pt {
            self.binary_hex.font_size_pt = v;
        }

        // header
        if let Some(v) = self.header_template.take() {
            self.header.template = v;
        }
        if let Some(v) = self.header_position {
            self.header.position = v;
        }
        if let Some(v) = self.header_rule {
            self.header.rule = v;
        }

        // footer
        if let Some(v) = self.footer_template.take() {
            self.footer.template = v;
        }
        if let Some(v) = self.footer_position {
            self.footer.position = v;
        }
        if let Some(v) = self.footer_rule {
            self.footer.rule = v;
        }

        // colophon
        if let Some(v) = self.colophon_template.take() {
            self.colophon.template = v;
        }

        // title page
        if let Some(v) = self.title_page_template.take() {
            self.title_page.template = v;
        }
        if let Some(v) = self.title_page_image.take() {
            self.title_page.image = v.to_string_lossy().to_string();
        }
        if let Some(v) = self.title_page_image_position {
            self.title_page.image_position = v;
        }
        if let Some(v) = self.title_page_image_max_height_in {
            self.title_page.image_max_height_in = v;
        }

        // metadata
        if let Some(v) = self.subject.take() {
            self.metadata.subject = v;
        }
        if let Some(v) = self.keywords.take() {
            self.metadata.keywords = v;
        }

        // numbering
        if let Some(v) = self.frontmatter_numbering {
            self.numbering.frontmatter = v;
        }
        if let Some(v) = self.source_numbering {
            self.numbering.source = v;
        }
        if let Some(v) = self.appendix_numbering {
            self.numbering.appendix = v;
        }
    }

    /// Returns the title page image path, if configured.
    pub fn title_page_image_path(&self) -> Option<PathBuf> {
        if self.title_page.image.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.title_page.image))
        }
    }

    /// Returns the booklet output path, if configured.
    pub fn booklet_outfile_path(&self) -> Option<PathBuf> {
        if self.booklet.outfile.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.booklet.outfile))
        }
    }

    /// Returns the subject, if configured.
    pub fn subject_opt(&self) -> Option<&str> {
        if self.metadata.subject.is_empty() {
            None
        } else {
            Some(&self.metadata.subject)
        }
    }

    /// Returns the keywords, if configured.
    pub fn keywords_opt(&self) -> Option<&str> {
        if self.metadata.keywords.is_empty() {
            None
        } else {
            Some(&self.metadata.keywords)
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

    #[test]
    fn can_roundtrip_pdf() {
        let pdf = PDF::default();
        let toml_str = toml::to_string(&pdf).expect("can serialize");
        let deserialized: PDF = toml::from_str(&toml_str).expect("can deserialize");
        assert_eq!(pdf.page.width_in, deserialized.page.width_in);
        assert_eq!(pdf.fonts.body_pt, deserialized.fonts.body_pt);
    }
}
