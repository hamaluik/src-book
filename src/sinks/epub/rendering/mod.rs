//! EPUB rendering orchestration.
//!
//! Coordinates the generation of all EPUB components: cover, TOC, source files,
//! commit history, and colophon. Uses the `epub-builder` crate which handles
//! the complex EPUB packaging requirements (OPF manifest, NCX navigation, ZIP
//! structure with proper MIME type). Each source file becomes a separate XHTML
//! document for efficient navigation on e-readers.

mod colophon;
mod commits;
mod cover;
mod source_file;
mod toc;

use super::config::{RenderStats, EPUB};
use super::styles;
use crate::source::{CommitOrder, Source};
use anyhow::{anyhow, Context, Result};
use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use indicatif::ProgressBar;
use std::fs::File;
use std::io::BufWriter;
use syntect::parsing::SyntaxSet;

/// Convert epub-builder's eyre::Report to anyhow::Error.
fn epub_err<T>(result: std::result::Result<T, eyre::Report>) -> Result<T> {
    result.map_err(|e| anyhow!("{}", e))
}

impl EPUB {
    /// Render the source repository to an EPUB file.
    ///
    /// Returns statistics about the generated EPUB.
    pub fn render(&self, source: &Source, progress: &ProgressBar) -> Result<RenderStats> {
        progress.set_message("Generating EPUB...");

        // load syntax highlighting assets
        let ss: SyntaxSet = bincode::serde::decode_from_slice(
            crate::highlight::SERIALIZED_SYNTAX,
            bincode::config::standard(),
        )
        .expect("can deserialise syntax set")
        .0;
        let theme = styles::load_theme(self.theme);

        // generate stylesheet
        let stylesheet = styles::generate_stylesheet(&theme, &self.fonts.family);

        // create epub builder
        let zip = epub_err(ZipLibrary::new())?;
        let mut builder = epub_err(EpubBuilder::new(zip))?;

        // set metadata
        let title = source.title.clone().unwrap_or_else(|| "Untitled".to_string());
        epub_err(builder.metadata("title", &title))?;
        epub_err(builder.metadata("generator", "src-book"))?;
        epub_err(builder.metadata("lang", &self.metadata.language))?;

        // add authors
        for author in &source.authors {
            epub_err(builder.metadata("author", author.to_string()))?;
        }

        // add optional metadata
        if let Some(subject) = self.subject_opt() {
            epub_err(builder.metadata("description", subject))?;
        }
        if let Some(keywords) = self.keywords_opt() {
            epub_err(builder.metadata("subject", keywords))?;
        }

        // add stylesheet
        epub_err(builder.stylesheet(stylesheet.as_bytes()))?;

        // track document count for stats
        let mut document_count = 0;

        // add cover page
        let cover_html = cover::render(self, source)?;
        epub_err(builder.add_content(
            EpubContent::new("cover.xhtml", cover_html.as_bytes())
                .title("Cover")
                .reftype(ReferenceType::Cover),
        ))?;
        document_count += 1;

        // add cover image if configured
        if let Some(cover_path) = self.cover_image_path() {
            let image_data = std::fs::read(&cover_path)
                .with_context(|| format!("Failed to read cover image: {}", cover_path.display()))?;
            let mime = mime_from_path(&cover_path);
            let filename = cover_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "cover-image".to_string());
            epub_err(builder.add_cover_image(&filename, image_data.as_slice(), mime))?;
        }

        // add colophon if configured
        if !self.colophon.template.is_empty() {
            let colophon_html = colophon::render(self, source)?;
            epub_err(builder.add_content(
                EpubContent::new("colophon.xhtml", colophon_html.as_bytes())
                    .title("Colophon")
                    .reftype(ReferenceType::Colophon),
            ))?;
            document_count += 1;
        }

        // add table of contents page
        let toc_html = toc::render(source)?;
        epub_err(builder.add_content(
            EpubContent::new("toc.xhtml", toc_html.as_bytes())
                .title("Table of Contents")
                .reftype(ReferenceType::Toc),
        ))?;
        document_count += 1;

        // add frontmatter files
        for (i, path) in source.frontmatter_files.iter().enumerate() {
            progress.inc(1);
            let filename = format!("frontmatter-{:04}.xhtml", i);
            let file_path = source.repository.join(path);
            let title = path.display().to_string();

            let html = source_file::render(&file_path, &title, &ss, &theme)?;
            epub_err(builder.add_content(EpubContent::new(&filename, html.as_bytes()).title(&title)))?;
            document_count += 1;
        }

        // add source files
        for (i, path) in source.source_files.iter().enumerate() {
            progress.inc(1);
            let filename = format!("source-{:04}.xhtml", i);
            let file_path = source.repository.join(path);
            let title = path.display().to_string();

            let html = source_file::render(&file_path, &title, &ss, &theme)?;
            epub_err(builder.add_content(EpubContent::new(&filename, html.as_bytes()).title(&title)))?;
            document_count += 1;
        }

        // add commit history if enabled
        if source.commit_order != CommitOrder::Disabled {
            let commits_html = commits::render(source)?;
            epub_err(builder.add_content(
                EpubContent::new("commits.xhtml", commits_html.as_bytes()).title("Commit History"),
            ))?;
            document_count += 1;
        }

        // write epub to file
        let output_file = File::create(&self.outfile)
            .with_context(|| format!("Failed to create EPUB file: {}", self.outfile.display()))?;
        let writer = BufWriter::new(output_file);
        epub_err(builder.generate(writer))?;

        progress.finish_with_message("EPUB generated");

        Ok(RenderStats { document_count })
    }
}

/// Determine MIME type from file extension.
fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}
