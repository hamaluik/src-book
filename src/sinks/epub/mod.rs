//! EPUB generation for source code books.
//!
//! This module converts a `Source` into an EPUB ebook with:
//! - Cover page with template placeholders and optional image
//! - Table of contents with hierarchical navigation
//! - Frontmatter files (README, LICENSE, etc.)
//! - Syntax-highlighted source files with line numbers
//! - Commit history (if enabled)
//! - Colophon with repository statistics and commit chart
//!
//! Uses the same syntax themes as PDF output for visual consistency.
//! CSS is generated from syntect themes to provide syntax highlighting
//! that matches the selected theme.

mod config;
mod rendering;
mod styles;

pub use config::EPUB;
