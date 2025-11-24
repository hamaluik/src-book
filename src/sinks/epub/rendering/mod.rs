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
use anyhow::{Context, Result};
use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use indicatif::ProgressBar;
use std::fs::File;
use std::io::BufWriter;
use syntect::parsing::SyntaxSet;

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
        let zip = ZipLibrary::new().with_context(|| "Failed to create ZIP library for EPUB")?;
        let mut builder = EpubBuilder::new(zip).with_context(|| "Failed to build builder")?;

        // set metadata
        // TODO: allow setting metadata to be fallible
        let title = source
            .title
            .clone()
            .unwrap_or_else(|| "Untitled".to_string());
        builder
            .metadata("title", &title)
            .with_context(|| "Failed to set title metadata")?;
        builder
            .metadata("generator", "src-book")
            .with_context(|| "Failed to set generator metadata")?;
        builder
            .metadata("lang", &self.metadata.language)
            .with_context(|| "Failed to set language metadata")?;

        // add authors
        for author in &source.authors {
            builder
                .metadata("author", author.to_string())
                .with_context(|| format!("Failed to add author metadata for author: {}", author))?;
        }

        // add optional metadata
        if let Some(subject) = self.subject_opt() {
            builder
                .metadata("description", subject)
                .with_context(|| "Failed to set description metadata")?;
        }
        if let Some(keywords) = self.keywords_opt() {
            builder
                .metadata("subject", keywords)
                .with_context(|| "Failed to set subject (keywords) metadata")?;
        }

        // add stylesheet
        builder
            .stylesheet(stylesheet.as_bytes())
            .with_context(|| "Failed to add stylesheet")?;

        // track document count for stats
        let mut document_count = 0;

        // add cover page
        let cover_html = cover::render(self, source)?;
        builder
            .add_content(
                EpubContent::new("cover.xhtml", cover_html.as_bytes())
                    .title("Cover")
                    .reftype(ReferenceType::Cover),
            )
            .with_context(|| "Failed to add cover page")?;
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
            builder
                .add_cover_image(&filename, image_data.as_slice(), mime)
                .with_context(|| {
                    format!(
                        "Failed to add cover image to EPUB: {}",
                        cover_path.display()
                    )
                })?;
        }

        // add colophon if configured
        if !self.colophon.template.is_empty() {
            let colophon_html = colophon::render(self, source)?;
            builder
                .add_content(
                    EpubContent::new("colophon.xhtml", colophon_html.as_bytes())
                        .title("Colophon")
                        .reftype(ReferenceType::Colophon),
                )
                .with_context(|| "Failed to add colophon page")?;
            document_count += 1;
        }

        // add table of contents page
        let toc_html = toc::render(source)?;
        builder
            .add_content(
                EpubContent::new("toc.xhtml", toc_html.as_bytes())
                    .title("Table of Contents")
                    .reftype(ReferenceType::Toc),
            )
            .with_context(|| "Failed to add table of contents page")?;
        document_count += 1;

        // add frontmatter files
        for (i, path) in source.frontmatter_files.iter().enumerate() {
            progress.inc(1);
            let filename = format!("frontmatter-{:04}.xhtml", i);
            let file_path = source.repository.join(path);
            let title = path.display().to_string();

            let html = source_file::render(&file_path, &title, &ss, &theme)?;
            builder
                .add_content(EpubContent::new(&filename, html.as_bytes()).title(&title))
                .with_context(|| {
                    format!(
                        "Failed to add frontmatter file to EPUB: {}",
                        file_path.display()
                    )
                })?;
            document_count += 1;
        }

        // add source files
        for (i, path) in source.source_files.iter().enumerate() {
            progress.inc(1);
            let filename = format!("source-{:04}.xhtml", i);
            let file_path = source.repository.join(path);
            let title = path.display().to_string();

            let html = source_file::render(&file_path, &title, &ss, &theme)?;
            builder
                .add_content(EpubContent::new(&filename, html.as_bytes()).title(&title))
                .with_context(|| {
                    format!("Failed to add source file to EPUB: {}", file_path.display())
                })?;
            document_count += 1;
        }

        // add commit history if enabled
        if source.commit_order != CommitOrder::Disabled {
            let commits_html = commits::render(source)?;
            builder
                .add_content(
                    EpubContent::new("commits.xhtml", commits_html.as_bytes())
                        .title("Commit History"),
                )
                .with_context(|| "Failed to add commit history page")?;
            document_count += 1;
        }

        // write epub to file
        let output_file = File::create(&self.outfile)
            .with_context(|| format!("Failed to create EPUB file: {}", self.outfile.display()))?;
        let writer = BufWriter::new(output_file);
        builder
            .generate(writer)
            .with_context(|| "Failed to generate EPUB file")?;

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
