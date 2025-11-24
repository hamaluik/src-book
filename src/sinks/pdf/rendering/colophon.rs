//! Colophon/statistics page rendering.
//!
//! Creates a page with book metadata, repository statistics, and commit activity.
//! The colophon appears after the title page and serves as the book's "about" page,
//! similar to the copyright/attribution page in traditional books.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::source::{Commit, Source};
use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use pdf_gen::*;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Statistics computed from the repository for display in the colophon.
#[derive(Debug, Default)]
pub struct ColophonStats {
    /// Total number of source files
    pub file_count: usize,
    /// Total lines of code across all source files
    pub line_count: usize,
    /// Total size in bytes of all source files
    pub total_bytes: u64,
    /// File and line counts grouped by extension
    pub language_stats: Vec<LanguageStat>,
    /// Total number of commits
    pub commit_count: usize,
    /// Date of the oldest commit
    pub first_commit: Option<NaiveDate>,
    /// Date of the newest commit
    pub last_commit: Option<NaiveDate>,
    /// Commit counts per month for the histogram (year-month string, count)
    pub commit_frequency: Vec<(String, u32)>,
}

/// Statistics for a single language/extension.
#[derive(Debug)]
pub struct LanguageStat {
    /// File extension (without the dot)
    pub extension: String,
    /// Number of files with this extension
    pub file_count: usize,
    /// Total lines in files with this extension
    pub line_count: usize,
}

/// Compute statistics from the source repository.
pub fn compute_stats(source: &Source, commits: &[Commit]) -> ColophonStats {
    let mut stats = ColophonStats::default();
    let mut lang_map: HashMap<String, (usize, usize)> = HashMap::new();

    for file in &source.source_files {
        let full_path = source.repository.join(file);

        // get file extension
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "other".to_string());

        // count file
        stats.file_count += 1;

        // count bytes
        if let Ok(metadata) = fs::metadata(&full_path) {
            stats.total_bytes += metadata.len();
        }

        // count lines (skip binary files)
        let lines = count_lines(&full_path).unwrap_or(0);
        stats.line_count += lines;

        // aggregate by extension
        let entry = lang_map.entry(ext).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += lines;
    }

    // convert language map to sorted vec (by line count, descending)
    let mut language_stats: Vec<LanguageStat> = lang_map
        .into_iter()
        .map(|(extension, (file_count, line_count))| LanguageStat {
            extension,
            file_count,
            line_count,
        })
        .collect();
    language_stats.sort_by(|a, b| b.line_count.cmp(&a.line_count));
    stats.language_stats = language_stats;

    // compute commit statistics
    stats.commit_count = commits.len();

    if !commits.is_empty() {
        // commits are already sorted (newest or oldest first), find the extremes
        let dates: Vec<NaiveDate> = commits
            .iter()
            .map(|c| c.date.date_naive())
            .collect();

        stats.first_commit = dates.iter().min().copied();
        stats.last_commit = dates.iter().max().copied();

        // compute commit frequency by month
        let mut month_counts: HashMap<String, u32> = HashMap::new();
        for commit in commits {
            let date = commit.date.date_naive();
            let key = format!("{:04}-{:02}", date.year(), date.month());
            *month_counts.entry(key).or_insert(0) += 1;
        }

        // sort by date and convert to vec
        let mut freq: Vec<(String, u32)> = month_counts.into_iter().collect();
        freq.sort_by(|a, b| a.0.cmp(&b.0));
        stats.commit_frequency = freq;
    }

    stats
}

/// Count lines in a file, returning 0 for binary files.
fn count_lines(path: &Path) -> Result<usize> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut count = 0;
    let mut is_likely_binary = false;

    for (i, line_result) in reader.lines().enumerate() {
        match line_result {
            Ok(line) => {
                count += 1;
                // check first few lines for binary content
                if i < 10 && line.contains('\0') {
                    is_likely_binary = true;
                    break;
                }
            }
            Err(_) => {
                // likely binary file, stop counting
                is_likely_binary = true;
                break;
            }
        }
    }

    if is_likely_binary {
        Ok(0)
    } else {
        Ok(count)
    }
}

/// Format bytes as a human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Generate a text-based commit frequency histogram.
fn render_commit_chart(frequency: &[(String, u32)]) -> String {
    if frequency.is_empty() {
        return String::new();
    }

    let max_count = frequency.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let bar_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    // limit to last 24 months for readability
    let display_freq: Vec<_> = if frequency.len() > 24 {
        frequency.iter().skip(frequency.len() - 24).collect()
    } else {
        frequency.iter().collect()
    };

    let mut lines = Vec::new();

    for (month, count) in display_freq {
        let bar_level = if max_count > 0 {
            (((*count as f64 / max_count as f64) * 7.0).round() as usize).min(7)
        } else {
            0
        };

        // create a bar with multiple characters for better visibility
        let bar_width = (((*count as f64 / max_count as f64) * 20.0).round() as usize).max(1);
        let bar: String = std::iter::repeat_n(bar_chars[bar_level], bar_width).collect();

        lines.push(format!("  {} {} ({})", month, bar, count));
    }

    lines.join("\n")
}

/// Format language statistics as a table.
fn render_language_stats(stats: &[LanguageStat]) -> String {
    if stats.is_empty() {
        return String::new();
    }

    // limit to top 10 languages
    let display_stats: Vec<_> = stats.iter().take(10).collect();

    let mut lines = vec!["Languages:".to_string()];

    for stat in display_stats {
        let ext = if stat.extension.is_empty() {
            "(no ext)"
        } else {
            &stat.extension
        };
        lines.push(format!(
            "  .{:<10} {:>5} files  {:>7} lines",
            ext, stat.file_count, stat.line_count
        ));
    }

    lines.join("\n")
}

/// Get all git remotes as a formatted string.
///
/// Returns lines in the format "name: url", one per remote.
/// Returns an empty string if no remotes exist or the repository cannot be opened.
fn get_remotes(repo_path: &Path) -> String {
    let Ok(repo) = git2::Repository::open(repo_path) else {
        return String::new();
    };

    let Ok(remotes) = repo.remotes() else {
        return String::new();
    };

    let mut lines = Vec::new();
    for name in remotes.iter().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            if let Some(url) = remote.url() {
                lines.push(format!("{}: {}", name, url));
            }
        }
    }

    lines.join("\n")
}

/// Expand template placeholders with actual values.
pub fn expand_template(template: &str, source: &Source, stats: &ColophonStats) -> String {
    let title = source.title.clone().unwrap_or_else(|| "untitled".to_string());

    let mut authors = source.authors.clone();
    authors.sort();
    let authors_str = authors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    let licences = if source.licences.is_empty() {
        "No licence specified".to_string()
    } else {
        source.licences.join(", ")
    };

    let generated_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let tool_version = env!("CARGO_PKG_VERSION");

    let date_range = match (stats.first_commit, stats.last_commit) {
        (Some(first), Some(last)) => format!("{} to {}", first, last),
        _ => "unknown".to_string(),
    };

    let language_stats = render_language_stats(&stats.language_stats);
    let commit_chart = render_commit_chart(&stats.commit_frequency);
    let remotes = get_remotes(&source.repository);

    template
        .replace("{title}", &title)
        .replace("{authors}", &authors_str)
        .replace("{licences}", &licences)
        .replace("{remotes}", &remotes)
        .replace("{generated_date}", &generated_date)
        .replace("{tool_version}", tool_version)
        .replace("{file_count}", &stats.file_count.to_string())
        .replace("{line_count}", &stats.line_count.to_string())
        .replace("{total_bytes}", &format_bytes(stats.total_bytes))
        .replace("{commit_count}", &stats.commit_count.to_string())
        .replace("{date_range}", &date_range)
        .replace("{language_stats}", &language_stats)
        .replace("{commit_chart}", &commit_chart)
}

/// Render the colophon page(s).
///
/// Returns the number of pages rendered (0 if colophon is disabled).
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    source: &Source,
    stats: &ColophonStats,
) -> Result<usize> {
    if config.colophon.template.is_empty() {
        return Ok(0);
    }

    let content = expand_template(&config.colophon.template, source, stats);
    let lines: Vec<&str> = content.lines().collect();

    let page_size = config.page_size();
    let font_size = Pt(config.fonts.body_pt);
    let small_size = Pt(config.fonts.small_pt);
    let line_height = doc.fonts[font_ids.regular].line_height(font_size);
    let small_line_height = doc.fonts[font_ids.regular].line_height(small_size);

    // calculate usable area
    let margin_top = Pt(config.margins.top_in * 72.0);
    let margin_bottom = Pt(config.margins.bottom_in * 72.0);
    let margin_left = Pt(config.margins.inner_in * 72.0);
    let usable_height = page_size.1 - margin_top - margin_bottom;

    // centre content vertically on the first page
    let total_height = Pt(lines.len() as f32) * line_height;
    let start_y = if total_height < usable_height {
        // centre vertically
        page_size.1 - margin_top - (usable_height - total_height) / 2.0
    } else {
        page_size.1 - margin_top
    };

    let mut page = Page::new(page_size, None);
    let mut y = start_y;
    let mut page_count = 0;

    for line in lines {
        // check if we need a new page
        if y < margin_bottom + line_height {
            doc.add_page(page);
            page_count += 1;
            page = Page::new(page_size, None);
            y = page_size.1 - margin_top;
        }

        // determine if this line should use small font (for chart/stats)
        // language stats lines start with "  ." (two spaces then extension dot)
        // commit chart lines contain Unicode block characters
        let (current_font_size, current_line_height) =
            if line.starts_with("  ") && (line.contains('▁') || line.starts_with("  .")) {
                (small_size, small_line_height)
            } else {
                (font_size, line_height)
            };

        if !line.is_empty() {
            page.add_span(SpanLayout {
                text: line.to_string(),
                font: SpanFont {
                    id: font_ids.regular,
                    size: current_font_size,
                },
                colour: colours::BLACK,
                coords: (margin_left, y),
            });
        }

        y -= current_line_height;
    }

    // add the last page
    doc.add_page(page);
    page_count += 1;

    Ok(page_count)
}
