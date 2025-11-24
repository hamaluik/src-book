//! Source file rendering with syntax highlighting for EPUB.
//!
//! Each source file becomes a separate XHTML chapter. Syntax highlighting uses
//! syntect with a hybrid styling approach: inline RGB colours ensure accurate
//! colour rendering regardless of e-reader CSS support, while CSS classes handle
//! bold/italic/underline styling for cleaner markup. Binary files show a placeholder
//! since hex dumps aren't practical in reflowable e-reader formats.

use crate::sinks::epub::styles;
use anyhow::{Context, Result};
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Render a source file as syntax-highlighted XHTML.
pub fn render(path: &Path, title: &str, ss: &SyntaxSet, theme: &Theme) -> Result<String> {
    let prefix = styles::scope_prefix();

    // read file contents
    let (contents, _is_binary) = match std::fs::read_to_string(path) {
        Ok(contents) => (contents.replace('\t', "    "), false),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            // binary file
            return Ok(render_binary_placeholder(title));
        }
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to read {}", path.display()));
        }
    };

    // find syntax for highlighting
    let syntax = ss.find_syntax_by_extension(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default(),
    );

    let code_html = if let Some(syntax) = syntax {
        // highlight with syntect
        let mut h = HighlightLines::new(syntax, theme);
        let mut html = String::new();

        for (line_num, line) in LinesWithEndings::from(&contents).enumerate() {
            // line number
            html.push_str(&format!(
                r#"<span class="line-number">{:>4}</span>"#,
                line_num + 1
            ));

            // highlighted tokens
            let ranges = h
                .highlight_line(line, ss)
                .with_context(|| format!("Failed to highlight line {}", line_num + 1))?;

            for (style, text) in ranges {
                let class = scope_to_class(style.font_style, prefix);
                let escaped = html_escape::encode_text(text);

                // always use inline colour, add classes for bold/italic/underline
                if class.is_empty() {
                    html.push_str(&format!(
                        r#"<span style="color: rgb({}, {}, {})">{}</span>"#,
                        style.foreground.r, style.foreground.g, style.foreground.b, escaped
                    ));
                } else {
                    html.push_str(&format!(
                        r#"<span class="{}" style="color: rgb({}, {}, {})">{}</span>"#,
                        class, style.foreground.r, style.foreground.g, style.foreground.b, escaped
                    ));
                }
            }
        }
        html
    } else {
        // no syntax highlighting - plain text with line numbers
        let mut html = String::new();
        for (line_num, line) in contents.lines().enumerate() {
            html.push_str(&format!(
                r#"<span class="line-number">{:>4}</span>{}<br/>"#,
                line_num + 1,
                html_escape::encode_text(line)
            ));
        }
        html
    };

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>{title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<div class="source-header">{title}</div>
<pre><code>{code}</code></pre>
</body>
</html>"#,
        title = html_escape::encode_text(title),
        code = code_html,
    ))
}

/// Render a placeholder for binary files.
fn render_binary_placeholder(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>{title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<div class="source-header">{title}</div>
<p class="binary-placeholder">&lt;binary data&gt;</p>
</body>
</html>"#,
        title = html_escape::encode_text(title),
    )
}

/// Map font style to CSS class names.
fn scope_to_class(font_style: FontStyle, prefix: &str) -> String {
    let mut classes = Vec::new();

    if font_style.intersects(FontStyle::BOLD) {
        classes.push(format!("{}bold", prefix));
    }
    if font_style.intersects(FontStyle::ITALIC) {
        classes.push(format!("{}italic", prefix));
    }
    if font_style.intersects(FontStyle::UNDERLINE) {
        classes.push(format!("{}underline", prefix));
    }

    classes.join(" ")
}
