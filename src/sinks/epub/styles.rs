//! CSS generation for EPUB syntax highlighting.
//!
//! Generates CSS stylesheets from syntect themes. The generated CSS includes:
//!
//! - Base document styles for consistent layout across e-readers
//! - Scope-based syntax classes (e.g., `.syn-keyword`, `.syn-string`) derived from theme colours
//! - Font style utility classes (`.syn-bold`, `.syn-italic`, `.syn-underline`) for tokens
//!   with special styling
//!
//! The source file renderer uses a hybrid approach: inline RGB colours for all tokens
//! (since scope-to-class mapping is imperfect) plus CSS classes for font styling.
//! This ensures colours always render correctly while keeping font styling maintainable.

use super::super::pdf::SyntaxTheme;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};

/// CSS class prefix for syntax highlighting spans.
const SCOPE_PREFIX: &str = "syn-";

/// Generate a complete CSS stylesheet for the EPUB.
///
/// Includes base styles for the document structure plus theme-derived syntax
/// highlighting classes.
pub fn generate_stylesheet(theme: &Theme, font_family: &str) -> String {
    let mut css = String::with_capacity(8192);

    // base document styles
    css.push_str(&generate_base_styles(font_family, theme));

    // syntax highlighting classes
    css.push_str("\n/* Syntax highlighting */\n");
    css.push_str(&generate_syntax_classes(theme));

    css
}

/// Generate base document styles.
fn generate_base_styles(font_family: &str, theme: &Theme) -> String {
    let bg = theme
        .settings
        .background
        .unwrap_or(syntect::highlighting::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });
    let fg = theme
        .settings
        .foreground
        .unwrap_or(syntect::highlighting::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });

    format!(
        r#"/* Base styles */
body {{
    font-family: serif;
    line-height: 1.5;
    margin: 1em;
    background-color: rgb({bg_r}, {bg_g}, {bg_b});
    color: rgb({fg_r}, {fg_g}, {fg_b});
}}

h1 {{
    font-size: 2em;
    text-align: center;
    margin: 1em 0;
}}

h2 {{
    font-size: 1.5em;
    margin: 1em 0 0.5em;
    border-bottom: 1px solid #ccc;
}}

h3 {{
    font-size: 1.2em;
    margin: 1em 0 0.5em;
}}

/* Code blocks */
pre {{
    font-family: "{font_family}", "Source Code Pro", "Fira Mono", monospace;
    font-size: 0.85em;
    line-height: 1.4;
    overflow-x: auto;
    padding: 0.5em;
    background-color: rgb({bg_r}, {bg_g}, {bg_b});
    border: 1px solid #ddd;
    border-radius: 3px;
}}

code {{
    font-family: "{font_family}", "Source Code Pro", "Fira Mono", monospace;
    font-size: 0.9em;
}}

/* Line numbers */
.line-number {{
    color: #999;
    user-select: none;
    padding-right: 1em;
    text-align: right;
    display: inline-block;
    min-width: 3em;
}}

/* Table of contents */
.toc {{
    margin: 1em 0;
}}

.toc ol {{
    list-style-type: none;
    padding-left: 1em;
}}

.toc li {{
    margin: 0.3em 0;
}}

.toc a {{
    text-decoration: none;
    color: inherit;
}}

.toc a:hover {{
    text-decoration: underline;
}}

/* Cover page */
.cover {{
    text-align: center;
    padding: 2em 1em;
}}

.cover h1 {{
    font-size: 2.5em;
    margin-bottom: 1em;
}}

.cover .authors {{
    font-size: 1.2em;
    margin: 1em 0;
}}

.cover img {{
    max-width: 80%;
    max-height: 50vh;
    margin: 1em auto;
}}

/* Colophon */
.colophon {{
    margin: 2em 0;
}}

.colophon hr {{
    border: none;
    border-top: 1px solid #ccc;
    margin: 1.5em 0;
}}

.colophon .stats {{
    font-family: "{font_family}", monospace;
    padding-left: 1em;
}}

/* Commit history */
.commit {{
    margin: 1em 0;
    padding: 0.5em;
    border-left: 3px solid #ddd;
}}

.commit .hash {{
    font-family: "{font_family}", monospace;
    font-size: 0.9em;
    color: #666;
}}

.commit .message {{
    font-weight: bold;
}}

.commit .meta {{
    font-size: 0.9em;
    color: #666;
}}

/* Source file header */
.source-header {{
    background: #f5f5f5;
    padding: 0.5em 1em;
    margin-bottom: 0.5em;
    border-radius: 3px;
    font-family: "{font_family}", monospace;
    font-size: 0.9em;
}}

/* Binary file placeholder */
.binary-placeholder {{
    font-style: italic;
    color: #888;
    padding: 1em;
    text-align: center;
}}

"#,
        bg_r = bg.r,
        bg_g = bg.g,
        bg_b = bg.b,
        fg_r = fg.r,
        fg_g = fg.g,
        fg_b = fg.b,
        font_family = font_family,
    )
}

/// Generate CSS classes for syntax highlighting based on the theme.
///
/// Maps common syntect scope selectors to CSS classes.
fn generate_syntax_classes(theme: &Theme) -> String {
    let mut css = String::new();

    // scope -> CSS class name mappings
    // these cover the most common syntax scopes across languages
    let scope_mappings = [
        ("comment", "comment"),
        ("string", "string"),
        ("constant.numeric", "number"),
        ("constant.language", "constant"),
        ("constant.character", "char"),
        ("keyword", "keyword"),
        ("keyword.control", "control"),
        ("keyword.operator", "operator"),
        ("storage", "storage"),
        ("storage.type", "type"),
        ("entity.name.function", "function"),
        ("entity.name.class", "class"),
        ("entity.name.tag", "tag"),
        ("entity.other.attribute-name", "attribute"),
        ("variable", "variable"),
        ("variable.parameter", "parameter"),
        ("support.function", "builtin"),
        ("support.type", "builtin-type"),
        ("punctuation", "punctuation"),
        ("meta.preprocessor", "preprocessor"),
        ("markup.heading", "heading"),
        ("markup.bold", "bold"),
        ("markup.italic", "italic"),
        ("markup.list", "list"),
        ("markup.quote", "quote"),
        ("markup.raw", "raw"),
        ("invalid", "invalid"),
    ];

    for (scope_str, class_name) in scope_mappings {
        if let Some(style) = find_style_for_scope(theme, scope_str) {
            css.push_str(&format_css_rule(class_name, &style));
        }
    }

    // default text colour for spans without a specific scope match
    if let Some(fg) = theme.settings.foreground {
        css.push_str(&format!(
            ".{SCOPE_PREFIX}default {{ color: rgb({}, {}, {}); }}\n",
            fg.r, fg.g, fg.b
        ));
    }

    // font style classes (used when tokens have bold/italic styling)
    css.push_str(&format!(".{SCOPE_PREFIX}bold {{ font-weight: bold; }}\n"));
    css.push_str(&format!(
        ".{SCOPE_PREFIX}italic {{ font-style: italic; }}\n"
    ));
    css.push_str(&format!(
        ".{SCOPE_PREFIX}underline {{ text-decoration: underline; }}\n"
    ));

    css
}

/// Find the style for a given scope string in the theme.
fn find_style_for_scope(
    theme: &Theme,
    scope_str: &str,
) -> Option<syntect::highlighting::StyleModifier> {
    // parse the scope and find the best match
    let scope = syntect::parsing::Scope::new(scope_str).ok()?;
    let scope_stack = syntect::parsing::ScopeStack::from_vec(vec![scope]);

    // find the best matching item
    for item in &theme.scopes {
        for sel in &item.scope.selectors {
            if sel.does_match(scope_stack.as_slice()).is_some() {
                return Some(item.style);
            }
        }
    }

    None
}

/// Format a CSS rule for a syntax class.
fn format_css_rule(class_name: &str, style: &syntect::highlighting::StyleModifier) -> String {
    let mut props = Vec::new();

    if let Some(fg) = style.foreground {
        props.push(format!("color: rgb({}, {}, {})", fg.r, fg.g, fg.b));
    }

    if let Some(bg) = style.background {
        // only add background if it's noticeably different from default
        props.push(format!(
            "background-color: rgb({}, {}, {})",
            bg.r, bg.g, bg.b
        ));
    }

    if let Some(font_style) = style.font_style {
        if font_style.intersects(FontStyle::BOLD) {
            props.push("font-weight: bold".to_string());
        }
        if font_style.intersects(FontStyle::ITALIC) {
            props.push("font-style: italic".to_string());
        }
        if font_style.intersects(FontStyle::UNDERLINE) {
            props.push("text-decoration: underline".to_string());
        }
    }

    if props.is_empty() {
        String::new()
    } else {
        format!(
            ".{}{} {{ {} }}\n",
            SCOPE_PREFIX,
            class_name,
            props.join("; ")
        )
    }
}

/// Load a theme by name from the serialised theme set.
pub fn load_theme(theme: SyntaxTheme) -> Theme {
    let ts: ThemeSet = bincode::serde::decode_from_slice(
        crate::highlight::SERIALIZED_THEMES,
        bincode::config::standard(),
    )
    .expect("can deserialise theme set")
    .0;
    ts.themes
        .get(theme.name())
        .cloned()
        .expect("theme exists in set")
}

/// Returns the CSS class prefix used for syntax highlighting.
pub fn scope_prefix() -> &'static str {
    SCOPE_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_generate_stylesheet() {
        let theme = load_theme(SyntaxTheme::GitHub);
        let css = generate_stylesheet(&theme, "SourceCodePro");
        assert!(css.contains("body {"));
        assert!(css.contains("pre {"));
        assert!(css.contains(".syn-"));
    }

    #[test]
    fn can_load_all_themes() {
        for theme in SyntaxTheme::all() {
            let _ = load_theme(*theme);
        }
    }
}
