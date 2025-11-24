//! Cover page rendering for EPUB.
//!
//! Creates the book's cover page using a configurable template with placeholders
//! for title, authors, licences, and date. Supports an optional cover image.
//! The cover is marked with EPUB's cover reference type so e-readers display
//! it appropriately in library views.

use crate::sinks::epub::config::EPUB;
use crate::source::Source;
use anyhow::Result;
use jiff::Zoned;

/// Render the cover page as XHTML.
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
    let licences = source.licences.join(", ");
    let date = Zoned::now().strftime("%Y-%m-%d").to_string();

    // expand template
    let content = config
        .cover
        .template
        .replace("{title}", &title)
        .replace("{authors}", &authors)
        .replace("{licences}", &licences)
        .replace("{date}", &date);

    // convert to HTML paragraphs
    let body_html = content
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                "<br/>".to_string()
            } else {
                format!("<p>{}</p>", html_escape::encode_text(line))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // add cover image if configured
    let image_html = if let Some(path) = config.cover_image_path() {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "cover-image".to_string());
        format!(r#"<img src="{}" alt="Cover"/>"#, filename)
    } else {
        String::new()
    };

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{lang}">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>{title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<div class="cover">
{image_html}
{body_html}
</div>
</body>
</html>"#,
        lang = config.metadata.language,
        title = html_escape::encode_text(&title),
        image_html = image_html,
        body_html = body_html,
    ))
}
