//! Colophon/statistics page rendering for EPUB.
//!
//! The colophon serves as the book's "about" page, displaying repository metadata,
//! generation info, and computed statistics. It mirrors the PDF colophon's content
//! and uses the same template placeholders for consistency. The commit activity
//! histogram uses graduated Unicode block characters to visualise contribution
//! patterns over time.

use crate::sinks::epub::config::EPUB;
use crate::source::{CommitOrder, Source};
use anyhow::Result;
use std::collections::HashMap;

/// Render the colophon page as XHTML.
pub fn render(config: &EPUB, source: &Source) -> Result<String> {
    let title = source
        .title
        .clone()
        .unwrap_or_else(|| "Untitled".to_string());
    let authors = source
        .authors
        .iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let licences = if source.licences.is_empty() {
        "No licence specified".to_string()
    } else {
        source.licences.join(", ")
    };

    // get git remotes
    let remotes = get_remotes(&source.repository);

    // compute statistics
    let stats = compute_stats(source);

    // expand template
    let content = config
        .colophon
        .template
        .replace("{title}", &title)
        .replace("{authors}", &authors)
        .replace("{licences}", &licences)
        .replace("{remotes}", &remotes)
        .replace(
            "{generated_date}",
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
        )
        .replace("{tool_version}", env!("CARGO_PKG_VERSION"))
        .replace("{file_count}", &stats.file_count.to_string())
        .replace("{line_count}", &format_number(stats.line_count))
        .replace("{total_bytes}", &format_bytes(stats.total_bytes))
        .replace("{commit_count}", &stats.commit_count.to_string())
        .replace("{date_range}", &stats.date_range)
        .replace("{language_stats}", &stats.language_stats)
        .replace("{commit_chart}", &stats.commit_chart);

    // convert to HTML
    let body_html = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                "<br/>".to_string()
            } else if line.starts_with("---") || line.starts_with("───") {
                "<hr/>".to_string()
            } else if line.starts_with("  ") {
                // indented lines go in stats div
                format!(
                    r#"<div class="stats">{}</div>"#,
                    html_escape::encode_text(line)
                )
            } else {
                format!("<p>{}</p>", html_escape::encode_text(line))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{lang}">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>Colophon - {title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<div class="colophon">
{body}
</div>
</body>
</html>"#,
        lang = config.metadata.language,
        title = html_escape::encode_text(&title),
        body = body_html,
    ))
}

/// Get git remote URLs from repository.
fn get_remotes(repo_path: &std::path::Path) -> String {
    let repo = match git2::Repository::open(repo_path) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    let remotes = match repo.remotes() {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    let mut urls = Vec::new();
    for name in remotes.iter().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            if let Some(url) = remote.url() {
                urls.push(url.to_string());
            }
        }
    }

    urls.join("\n")
}

struct ColophonStats {
    file_count: usize,
    line_count: usize,
    total_bytes: u64,
    commit_count: usize,
    date_range: String,
    language_stats: String,
    commit_chart: String,
}

fn compute_stats(source: &Source) -> ColophonStats {
    let mut file_count = 0;
    let mut line_count = 0;
    let mut total_bytes = 0u64;
    let mut language_lines: HashMap<String, usize> = HashMap::new();

    // count source files
    for path in &source.source_files {
        let full_path = source.repository.join(path);
        if let Ok(metadata) = std::fs::metadata(&full_path) {
            total_bytes += metadata.len();
        }
        if let Ok(contents) = std::fs::read_to_string(&full_path) {
            let lines = contents.lines().count();
            line_count += lines;

            // track by extension
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("other")
                .to_string();
            *language_lines.entry(ext).or_default() += lines;
        }
        file_count += 1;
    }

    // format language stats
    let mut langs: Vec<_> = language_lines.into_iter().collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1));
    let language_stats = langs
        .iter()
        .take(10)
        .map(|(ext, lines)| format!("  .{}: {} lines", ext, format_number(*lines)))
        .collect::<Vec<_>>()
        .join("\n");

    // get commit info
    let commits = if source.commit_order != CommitOrder::Disabled {
        source.commits().unwrap_or_default()
    } else {
        vec![]
    };
    let commit_count = commits.len();

    // date range
    let date_range = if commits.is_empty() {
        "no commits".to_string()
    } else {
        let first = commits.last().map(|c| c.date.format("%Y-%m-%d").to_string());
        let last = commits.first().map(|c| c.date.format("%Y-%m-%d").to_string());
        match (first, last) {
            (Some(f), Some(l)) if f != l => format!("{} to {}", f, l),
            (Some(f), _) => f,
            _ => "unknown".to_string(),
        }
    };

    // commit chart (simplified text version)
    let commit_chart = generate_commit_chart(&commits);

    ColophonStats {
        file_count,
        line_count,
        total_bytes,
        commit_count,
        date_range,
        language_stats,
        commit_chart,
    }
}

fn generate_commit_chart(commits: &[crate::source::Commit]) -> String {
    if commits.is_empty() {
        return "  (no commits)".to_string();
    }

    // group by month
    let mut monthly: HashMap<String, usize> = HashMap::new();
    for commit in commits {
        let key = commit.date.format("%Y-%m").to_string();
        *monthly.entry(key).or_default() += 1;
    }

    let mut months: Vec<_> = monthly.into_iter().collect();
    months.sort();

    // take last 12 months
    let months: Vec<_> = months.into_iter().rev().take(12).rev().collect();

    if months.is_empty() {
        return "  (no commits)".to_string();
    }

    let max_commits = months.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let bar_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    months
        .iter()
        .map(|(month, count)| {
            let bar_level = if max_commits > 0 {
                (((*count as f64 / max_commits as f64) * 7.0).round() as usize).min(7)
            } else {
                0
            };
            let bar_width = (((*count as f64 / max_commits as f64) * 20.0).round() as usize).max(1);
            let bar: String = std::iter::repeat_n(bar_chars[bar_level], bar_width).collect();
            format!("  {} {:>3} {}", month, count, bar)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// use shared formatting utilities
use crate::formatting::{format_bytes, format_number};
