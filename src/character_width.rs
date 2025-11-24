//! Layout capacity calculations for printed source code.
//!
//! This module provides utilities to calculate how many characters can fit on a line
//! in the rendered PDF, using actual font glyph metrics rather than rough estimates.
//! This is critical for helping users understand whether their source code will remain
//! readable or will wrap awkwardly when printed.
//!
//! The calculations account for all layout constraints:
//! - Page dimensions (width in inches)
//! - Left and right margins (in points)
//! - Line number space (6 characters reserved for syntax highlighting: "1234  ")
//! - Actual font metrics (using the widest glyph 'M' for conservative estimates)
//!
//! # Why This Matters
//!
//! Source code readability in print depends heavily on line length. Code that wraps
//! mid-expression becomes difficult to follow. By showing users the characters-per-line
//! capacity during configuration, they can make informed decisions about page size and
//! font size before generating their book. This prevents the frustration of discovering
//! after rendering that their 120-character lines are being wrapped.

use pdf_gen::layout::width_of_text;
use pdf_gen::{Font, Pt};

/// Calculates the maximum number of characters that can fit on a single line
/// for the given page layout and font settings.
///
/// This uses actual glyph metrics to provide an accurate character count,
/// helping users understand if their source code will wrap or break when printed.
///
/// **Important**: This accounts for line numbers in syntax-highlighted files.
/// Source files with syntax highlighting reserve 6 characters (4-digit line number + 2 spaces)
/// at the start of each line, reducing the available width for code.
///
/// # Parameters
/// - `page_width_in`: Page width in inches
/// - `left_margin_pt`: Left margin in points
/// - `right_margin_pt`: Right margin in points
/// - `font`: The font to measure (should be the regular variant for body text)
/// - `font_size_pt`: Font size in points
///
/// # Returns
/// The maximum number of characters that fit within the available text width,
/// accounting for line numbers. Uses uppercase 'M' as the reference character
/// since it's typically the widest in monospace fonts, providing a conservative
/// (worst-case) estimate.
pub fn calculate_max_chars_per_line(
    page_width_in: f32,
    left_margin_pt: f32,
    right_margin_pt: f32,
    font: &Font,
    font_size_pt: f32,
) -> usize {
    let page_width_pt = page_width_in * 72.0;
    let available_width_pt = page_width_pt - left_margin_pt - right_margin_pt;

    // measure the width of a single 'M' character (typically widest in monospace fonts)
    let single_char_width = width_of_text("M", font, Pt(font_size_pt));

    // account for line numbers: syntax-highlighted files use "{:>4}  " format (6 chars)
    let line_number_width = width_of_text("      ", font, Pt(font_size_pt));
    let code_width_pt = available_width_pt - line_number_width.0;

    // calculate how many characters fit in the remaining space
    (code_width_pt / single_char_width.0).floor() as usize
}
