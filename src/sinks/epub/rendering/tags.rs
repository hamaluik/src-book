//! Git tags appendix rendering for EPUB.
//!
//! Displays all tags with their commit info, optionally including tagger
//! and message for annotated tags.

use crate::source::Tag;
use anyhow::Result;

/// Render the tags appendix as XHTML.
pub fn render(title: &str, tags: &[Tag]) -> Result<String> {
    if tags.is_empty() {
        return Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>Tags - {title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<h2>Tags</h2>
<p>No tags found.</p>
</body>
</html>"#,
            title = html_escape::encode_text(title),
        ));
    }

    let tags_html: String = tags
        .iter()
        .map(|tag| {
            let hash_short = &tag.commit_hash[..8.min(tag.commit_hash.len())];
            let summary = tag
                .commit_summary
                .as_deref()
                .map(|s| html_escape::encode_text(s).to_string())
                .unwrap_or_default();
            let commit_date = tag.commit_date.strftime("%Y-%m-%d %H:%M");
            let tag_name = html_escape::encode_text(&tag.name);

            // build annotated tag extras
            let annotated_html = if tag.is_annotated {
                let mut parts = Vec::new();

                if let Some(tagger) = &tag.tagger {
                    let tagger_string = tagger.to_string();
                    let tagger_str = html_escape::encode_text(&tagger_string);
                    parts.push(format!(r#"<div class="tag-tagger">Tagged by: {}</div>"#, tagger_str));
                }

                if let Some(tag_date) = &tag.tag_date {
                    let date_str = tag_date.strftime("%Y-%m-%d %H:%M");
                    parts.push(format!(r#"<div class="tag-date">Tag date: {}</div>"#, date_str));
                }

                if let Some(message) = &tag.message {
                    let msg_escaped = html_escape::encode_text(message);
                    parts.push(format!(r#"<div class="tag-message">{}</div>"#, msg_escaped));
                }

                parts.join("\n")
            } else {
                String::new()
            };

            format!(
                r#"<div class="tag">
<span class="tag-name">{tag_name}</span> <span class="tag-arrow">→</span> <span class="tag-commit">{hash}</span>
<div class="tag-summary">{summary}</div>
<div class="tag-commit-date">{commit_date}</div>
{annotated}
</div>"#,
                tag_name = tag_name,
                hash = hash_short,
                summary = summary,
                commit_date = commit_date,
                annotated = annotated_html,
            )
        })
        .collect();

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>Tags - {title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<h2>Tags</h2>
<p>{count} tags</p>
{tags}
</body>
</html>"#,
        title = html_escape::encode_text(title),
        count = tags.len(),
        tags = tags_html,
    ))
}
