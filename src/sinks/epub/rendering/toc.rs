//! Table of contents rendering for EPUB.
//!
//! Generates a navigable TOC page with frontmatter listed first, followed by
//! source files in a hierarchical tree structure reflecting directory layout.
//! This complements the EPUB's built-in navigation (NCX/nav.xhtml) with a
//! human-readable page that readers can browse.

use crate::source::Source;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Render the table of contents as XHTML.
pub fn render(source: &Source) -> Result<String> {
    let title = source
        .title
        .clone()
        .unwrap_or_else(|| "Untitled".to_string());

    let mut toc_items = Vec::new();
    let mut file_index = 0;

    // frontmatter section
    if !source.frontmatter_files.is_empty() {
        toc_items.push("<h3>Frontmatter</h3>".to_string());
        toc_items.push("<ol>".to_string());
        for path in &source.frontmatter_files {
            let href = format!("frontmatter-{:04}.xhtml", file_index);
            let name = path.display().to_string();
            toc_items.push(format!(
                r#"<li><a href="{}">{}</a></li>"#,
                href,
                html_escape::encode_text(&name)
            ));
            file_index += 1;
        }
        toc_items.push("</ol>".to_string());
    }

    // source files section with hierarchy
    if !source.source_files.is_empty() {
        toc_items.push("<h3>Source Files</h3>".to_string());
        toc_items.push(render_hierarchical_toc(&source.source_files));
    }

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
<head>
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8"/>
    <title>Table of Contents - {title}</title>
    <link rel="stylesheet" type="text/css" href="stylesheet.css"/>
</head>
<body>
<div class="toc">
<h2>Table of Contents</h2>
{items}
</div>
</body>
</html>"#,
        title = html_escape::encode_text(&title),
        items = toc_items.join("\n"),
    ))
}

/// Render a hierarchical table of contents for source files.
fn render_hierarchical_toc(files: &[std::path::PathBuf]) -> String {
    // build directory tree
    let mut tree: HashMap<&Path, Vec<(usize, &Path)>> = HashMap::new();

    for (i, path) in files.iter().enumerate() {
        let parent = path.parent().unwrap_or(Path::new(""));
        tree.entry(parent).or_default().push((i, path.as_path()));
    }

    let mut html = String::new();
    html.push_str("<ol>");

    // render root level and recurse
    render_tree_level(&tree, Path::new(""), &mut html);

    html.push_str("</ol>");
    html
}

fn render_tree_level(
    tree: &HashMap<&Path, Vec<(usize, &Path)>>,
    current: &Path,
    html: &mut String,
) {
    // collect all directories at this level
    let mut subdirs: Vec<&Path> = tree
        .keys()
        .filter(|p| {
            p.parent() == Some(current)
                || (current.as_os_str().is_empty() && p.components().count() == 1)
        })
        .copied()
        .collect();
    subdirs.sort();

    // render files in current directory
    if let Some(files) = tree.get(current) {
        for (idx, path) in files {
            let href = format!("source-{:04}.xhtml", idx);
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            html.push_str(&format!(
                r#"<li><a href="{}">{}</a></li>"#,
                href,
                html_escape::encode_text(&name)
            ));
        }
    }

    // render subdirectories
    for subdir in subdirs {
        let dir_name = subdir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| subdir.display().to_string());
        html.push_str(&format!(
            "<li><strong>{}</strong><ol>",
            html_escape::encode_text(&dir_name)
        ));
        render_tree_level(tree, subdir, html);
        html.push_str("</ol></li>");
    }
}
