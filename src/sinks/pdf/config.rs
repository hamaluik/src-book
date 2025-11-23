use pdf_gen::Pt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

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

/// PDF output configuration.
///
/// Margins are asymmetric to support booklet printing: inner margins accommodate
/// binding, while outer margins can be smaller. Top margins are typically larger
/// than bottom to leave room for headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
