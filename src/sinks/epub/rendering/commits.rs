//! Commit history rendering for EPUB.
//!
//! Displays git commits with hash, message, author, and date. Each commit is
//! rendered as a styled div with CSS classes for consistent formatting.
//! Optionally displays tag badges inline with commits.

use crate::source::Source;
use anyhow::Result;
use std::collections::HashMap;

/// Render the commit history as XHTML.
///
/// If `tags_by_commit` is provided, tags pointing to each commit are rendered
/// as `[tag_name]` badges after the commit hash.
pub fn render(
    source: &Source,
    tags_by_commit: Option<&HashMap<String, Vec<String>>>,
) -> Result<String> {
    let title = source
        .title
        .clone()
        .unwrap_or_else(|| "Untitled".to_string());

    let commits = source.commits().unwrap_or_default();

    let commits_html: String = commits
        .iter()
        .map(|commit| {
            let hash_short = &commit.hash[..8.min(commit.hash.len())];
            let message = commit.summary.as_deref().unwrap_or("(no message)");
            let date = commit.date.strftime("%Y-%m-%d %H:%M");
            let author_str = commit.author.to_string();
            let author = html_escape::encode_text(&author_str);
            let message_escaped = html_escape::encode_text(message);

            // render inline tag badges if available
            let tags_html = if let Some(tags_map) = tags_by_commit {
                if let Some(tags) = tags_map.get(&commit.hash) {
                    tags.iter()
                        .map(|t| {
                            format!(
                                r#"<span class="tag-badge">[{}]</span>"#,
                                html_escape::encode_text(t)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let tags_span = if tags_html.is_empty() {
                String::new()
            } else {
                format!(" {}", tags_html)
            };

            format!(
                r#"<div class="commit">
<span class="hash">{hash}</span>{tags}
<div class="message">{message}</div>
<div class="meta">{author} &#183; {date}</div>
</div>"#,
                hash = hash_short,
                tags = tags_span,
                message = message_escaped,
                author = author,
                date = date,
            )
        })
        .collect();

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>Commit History - {title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<h2>Commit History</h2>
<p>{count} commits</p>
{commits}
</body>
</html>"#,
        title = html_escape::encode_text(&title),
        count = commits.len(),
        commits = commits_html,
    ))
}
