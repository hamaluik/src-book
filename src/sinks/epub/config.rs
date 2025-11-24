//! EPUB output configuration.
//!
//! Defines the configuration structs for EPUB generation, mirroring the PDF
//! configuration structure for consistency. Uses the same `SyntaxTheme` enum
//! as PDF so theme selection works uniformly across output formats.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::super::pdf::SyntaxTheme;

/// Cover page configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverConfig {
    /// Template with placeholders: {title}, {authors}, {licences}, {date}.
    pub template: String,
    /// Optional cover image path. Empty string for none.
    pub image: String,
}

impl Default for CoverConfig {
    fn default() -> Self {
        Self {
            template: default_cover_template(),
            image: String::new(),
        }
    }
}

/// Colophon/statistics page configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColophonConfig {
    /// Template with placeholders. Empty string disables the colophon page.
    /// Placeholders: {title}, {authors}, {licences}, {remotes}, {generated_date},
    /// {tool_version}, {file_count}, {line_count}, {total_bytes}, {language_stats},
    /// {commit_count}, {date_range}, {commit_chart}
    pub template: String,
}

impl Default for ColophonConfig {
    fn default() -> Self {
        Self {
            template: default_colophon_template(),
        }
    }
}

/// EPUB document metadata configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataConfig {
    /// Subject/description for EPUB metadata.
    /// Empty string for none.
    pub subject: String,
    /// Keywords for EPUB metadata (comma-separated recommended).
    /// Empty string for none.
    pub keywords: String,
    /// Language code (BCP 47 format, e.g., "en", "en-GB", "fr").
    /// Required for valid EPUB.
    pub language: String,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            subject: String::new(),
            keywords: String::new(),
            language: "en".to_string(),
        }
    }
}

/// Font embedding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontsConfig {
    /// Whether to embed fonts in the EPUB.
    pub embed: bool,
    /// Font family for code blocks ("SourceCodePro", "FiraMono", or path to custom font).
    pub family: String,
}

impl Default for FontsConfig {
    fn default() -> Self {
        Self {
            embed: true,
            family: "SourceCodePro".to_string(),
        }
    }
}

/// EPUB output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub struct EPUB {
    /// Output EPUB file path
    pub outfile: PathBuf,
    /// Syntax highlighting theme for code blocks
    pub theme: SyntaxTheme,

    /// Cover page configuration
    pub cover: CoverConfig,
    /// Colophon/statistics page configuration
    pub colophon: ColophonConfig,
    /// EPUB document metadata
    pub metadata: MetadataConfig,
    /// Font configuration
    pub fonts: FontsConfig,
}

impl Default for EPUB {
    fn default() -> Self {
        Self {
            outfile: PathBuf::from("book.epub"),
            theme: SyntaxTheme::GitHub,
            cover: CoverConfig::default(),
            colophon: ColophonConfig::default(),
            metadata: MetadataConfig::default(),
            fonts: FontsConfig::default(),
        }
    }
}

impl EPUB {
    /// Returns the cover image path, if configured.
    pub fn cover_image_path(&self) -> Option<PathBuf> {
        if self.cover.image.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.cover.image))
        }
    }

    /// Returns the subject, if configured.
    pub fn subject_opt(&self) -> Option<&str> {
        if self.metadata.subject.is_empty() {
            None
        } else {
            Some(&self.metadata.subject)
        }
    }

    /// Returns the keywords, if configured.
    pub fn keywords_opt(&self) -> Option<&str> {
        if self.metadata.keywords.is_empty() {
            None
        } else {
            Some(&self.metadata.keywords)
        }
    }
}

pub fn default_cover_template() -> String {
    r#"{title}

- by -

{authors}"#
        .to_string()
}

pub fn default_colophon_template() -> String {
    r#"{title}

by {authors}

{remotes}

---

Generated on {generated_date}
by src-book v{tool_version}

{licences}

---

Statistics

  {file_count} source files
  {line_count} lines of code
  {total_bytes}
  {commit_count} commits ({date_range})

{language_stats}

Commit Activity

{commit_chart}"#
        .to_string()
}

/// Statistics from rendering an EPUB, used for user feedback.
pub struct RenderStats {
    /// Number of documents/chapters in the EPUB
    pub document_count: usize,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_serialize_epub() {
        let epub = EPUB::default();
        toml::to_string(&epub).expect("can serialize EPUB to TOML");
    }

    #[test]
    fn can_roundtrip_epub() {
        let epub = EPUB::default();
        let toml_str = toml::to_string(&epub).expect("can serialize");
        let deserialized: EPUB = toml::from_str(&toml_str).expect("can deserialize");
        assert_eq!(
            epub.outfile.to_string_lossy(),
            deserialized.outfile.to_string_lossy()
        );
    }
}
