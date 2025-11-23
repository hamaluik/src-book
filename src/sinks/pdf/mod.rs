//! PDF generation for source code books.
//!
//! This module converts a `Source` into one or two PDFs:
//! - A digital PDF optimised for on-screen reading with clickable links and bookmarks
//! - An optional print-ready booklet PDF with saddle-stitch imposition
//!
//! The rendering process creates a title page, syntax-highlighted source files,
//! embedded images, commit history, and a table of contents. Headers and footers
//! are rendered on content pages using customisable templates with placeholders.

mod booklet;
mod config;
mod fonts;
mod imposition;
mod rendering;

pub use config::{PageSize, Position, RulePosition, SyntaxTheme, PDF};
