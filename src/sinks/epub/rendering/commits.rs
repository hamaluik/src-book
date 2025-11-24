//! Commit history rendering for EPUB.
//!
//! Displays git commits with hash, message, author, and date. Each commit is
//! rendered as a styled div with CSS classes for consistent formatting.

use crate::source::Source;
use anyhow::Result;

/// Render the commit history as XHTML.
pub fn render(source: &Source) -> Result<String> {
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

            format!(
                r#"<div class="commit">
<span class="hash">{hash}</span>
<div class="message">{message}</div>
<div class="meta">{author} &#183; {date}</div>
</div>"#,
                hash = hash_short,
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
