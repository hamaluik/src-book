//! Source file line length analysis and font size optimisation.
//!
//! This module provides tools to analyse actual source code line lengths and suggest
//! optimal font sizes for readable printed output. It complements `character_width`'s
//! theoretical calculations by measuring real line lengths in the repository.
//!
//! # Why This Matters
//!
//! While `character_width` tells users how many characters *could* fit on a line,
//! this module tells them how many characters their code *actually* uses. By scanning
//! source files and measuring line lengths, the tool can:
//!
//! - Calculate what percentage of lines would wrap at the current settings
//! - Find the longest line and where it occurs
//! - Determine the 95th percentile line length for sensible optimisation targets
//! - Suggest font size reductions that would fit most lines without wrapping
//!
//! The interactive wizard uses this module to create a feedback loop: show initial
//! capacity, scan files, display wrapping statistics, suggest adjustments, re-scan
//! with new settings, repeat until the user is satisfied. This prevents the frustration
//! of discovering after a 10-minute render that all your code is wrapped and unreadable.
//!
//! # Implementation Details
//!
//! Line length measurement treats tabs as 4 spaces (matching common editor behaviour)
//! and counts characters rather than bytes. The 95th percentile is used as the
//! optimisation target rather than the maximum because a few extremely long lines
//! (often comments or generated code) shouldn't force the entire book to use tiny fonts.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

/// Statistics about line lengths in source files
#[derive(Debug)]
pub struct LineStats {
    pub total_lines: usize,
    pub lines_that_wrap: usize,
    pub longest_line_length: usize,
    pub longest_line_file: PathBuf,
    pub longest_line_number: usize,
    pub percentile_95: usize,
}

impl LineStats {
    /// Calculates the percentage of lines that would wrap at the current character limit.
    ///
    /// Returns 0.0 if there are no lines to analyse.
    pub fn wrap_percentage(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.lines_that_wrap as f64 / self.total_lines as f64) * 100.0
        }
    }
}

/// Analyses line lengths in source files and reports wrapping statistics.
///
/// Reads all source files, measures visual line length (treating tabs as 4 spaces),
/// and calculates statistics about how many lines would wrap given the max_chars_per_line.
///
/// Binary files (those that can't be read as UTF-8) are silently skipped, matching the
/// behaviour of the EPUB renderer.
///
/// # Parameters
///
/// - `source_files`: Paths to source files relative to the repository root
/// - `repository_path`: Absolute path to the repository root directory
/// - `max_chars_per_line`: Character limit to check against
///
/// # Returns
///
/// Statistics including total lines, lines that wrap, longest line location, and
/// 95th percentile line length. The 95th percentile is particularly useful as an
/// optimisation target since it ignores outliers (e.g., extremely long generated lines)
/// that would otherwise force unnecessarily small fonts.
pub fn analyze_line_lengths(
    source_files: &[PathBuf],
    repository_path: &Path,
    max_chars_per_line: usize,
) -> Result<LineStats> {
    let pb = ProgressBar::new(source_files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("can create progress style")
            .progress_chars("#>-"),
    );
    pb.set_message("Analyzing files...");

    let mut total_lines = 0;
    let mut lines_that_wrap = 0;
    let mut longest_line_length = 0;
    let mut longest_line_file = PathBuf::new();
    let mut longest_line_number = 0;
    let mut all_line_lengths = Vec::new();

    for file_path in source_files {
        pb.inc(1);

        let full_path = repository_path.join(file_path);

        // skip binary files by attempting to read as UTF-8
        // same approach as EPUB renderer
        let Ok(contents) = std::fs::read_to_string(&full_path) else {
            // binary file or unreadable, skip it
            continue;
        };

        // process each line
        for (line_num, line) in contents.lines().enumerate() {
            total_lines += 1;

            // calculate visual width (tabs = 4 spaces)
            let visual_length = line
                .chars()
                .fold(0, |acc, c| if c == '\t' { acc + 4 } else { acc + 1 });

            all_line_lengths.push(visual_length);

            if visual_length > max_chars_per_line {
                lines_that_wrap += 1;
            }

            if visual_length > longest_line_length {
                longest_line_length = visual_length;
                longest_line_file = file_path.clone();
                longest_line_number = line_num + 1; // 1-indexed for display
            }
        }
    }

    pb.finish_and_clear();

    // calculate 95th percentile
    all_line_lengths.sort_unstable();
    let percentile_95_idx = (all_line_lengths.len() as f64 * 0.95) as usize;
    let percentile_95 = all_line_lengths
        .get(percentile_95_idx)
        .copied()
        .unwrap_or(0);

    Ok(LineStats {
        total_lines,
        lines_that_wrap,
        longest_line_length,
        longest_line_file,
        longest_line_number,
        percentile_95,
    })
}

/// Calculates the suggested font size to fit a target line length.
///
/// Uses binary search and the same glyph width calculation as `character_width.rs`
/// to determine what font size would allow `target_chars` characters to fit within
/// the available page width.
///
/// # Parameters
///
/// - `page_width_in`: Page width in inches
/// - `inner_margin_in`: Inner (binding) margin in inches
/// - `outer_margin_in`: Outer margin in inches
/// - `target_chars`: Desired number of characters per line (typically the 95th percentile)
/// - `font`: The font to measure (should be regular variant)
///
/// # Returns
///
/// The optimal font size in points, rounded to 1 decimal place. The returned size will
/// fit `target_chars` within the page width while accounting for margins and the 6-character
/// line number space.
///
/// # Algorithm
///
/// Uses binary search between 1.0 and 72.0 points with 0.1pt precision. At each iteration,
/// measures how many characters would fit at that font size. If the target fits, tries larger
/// sizes; if it doesn't fit, tries smaller sizes. This converges quickly (typically 10-15
/// iterations) while providing accurate results.
pub fn calculate_suggested_font_size(
    page_width_in: f64,
    inner_margin_in: f64,
    outer_margin_in: f64,
    target_chars: usize,
    font: &pdf_gen::Font,
) -> f64 {
    use pdf_gen::layout::width_of_text;
    use pdf_gen::Pt;

    // convert inches to points (1 inch = 72 points)
    let page_width_pt = page_width_in * 72.0;
    let inner_margin_pt = inner_margin_in * 72.0;
    let outer_margin_pt = outer_margin_in * 72.0;

    // available width for text (accounting for both margins)
    let text_width_pt = page_width_pt - inner_margin_pt - outer_margin_pt;

    // we need to find font size such that (target_chars + 6 line number chars) fit
    // use binary search approach: try different font sizes until we find one that fits

    // start with a guess based on current capacity and target
    // use a simple linear approximation as starting point
    let mut low: f64 = 1.0;
    let mut high: f64 = 72.0; // reasonable upper bound for font size
    let mut best_size: f64 = 8.0;

    // binary search for font size (precision to 0.1pt)
    while (high - low) > 0.1 {
        let mid = (low + high) / 2.0;

        // measure width at this font size
        let single_char_width = width_of_text("M", font, Pt(mid as f32));
        let line_number_width = width_of_text("      ", font, Pt(mid as f32));
        let code_width_pt = text_width_pt as f32 - line_number_width.0;
        let chars_that_fit = (code_width_pt / single_char_width.0).floor() as usize;

        if chars_that_fit >= target_chars {
            // this size fits, try larger
            best_size = mid;
            low = mid;
        } else {
            // too big, try smaller
            high = mid;
        }
    }

    // round to 1 decimal place
    let rounded: f64 = (best_size * 10.0).round() / 10.0;
    rounded
}
