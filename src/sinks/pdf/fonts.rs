use anyhow::{anyhow, Context, Result};
use pdf_gen::id_arena_crate::Id;
use pdf_gen::Font;
use std::path::{Path, PathBuf};

/// Font IDs for the document, populated during render.
///
/// Syntax highlighting requires all four variants to properly render bold, italic, and
/// bold-italic code tokens. Fonts that lack certain variants (like FiraMono which has
/// no italic) fall back to regular/bold as appropriate.
pub struct FontIds {
    pub regular: Id<Font>,
    pub bold: Id<Font>,
    pub italic: Id<Font>,
    pub bold_italic: Id<Font>,
}

/// Loaded font data before being added to the document.
///
/// Supports three loading modes:
/// - "SourceCodePro": bundled font with full variant support
/// - "FiraMono": bundled font with Regular/Bold only (italic falls back)
/// - "./path/to/Font": custom font loaded from disk using naming conventions
pub struct LoadedFonts {
    pub regular: Font,
    pub bold: Font,
    pub italic: Font,
    pub bold_italic: Font,
}

impl LoadedFonts {
    /// Load fonts based on font name configuration.
    ///
    /// Supports:
    /// - "SourceCodePro" - bundled font with all 4 variants
    /// - "FiraMono" - bundled font (Regular/Bold only, falls back for italic)
    /// - Path like "./fonts/MyFont" - loads MyFont-Regular.ttf, MyFont-Bold.ttf, etc.
    pub fn load(font_name: &str) -> Result<LoadedFonts> {
        match font_name {
            "SourceCodePro" => Self::load_source_code_pro(),
            "FiraMono" => Self::load_fira_mono(),
            _ => Self::load_from_path(font_name),
        }
    }

    fn load_source_code_pro() -> Result<LoadedFonts> {
        let regular =
            Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-Regular.ttf").to_vec())
                .with_context(|| "Failed to load SourceCodePro-Regular.ttf")?;
        let bold =
            Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-Bold.ttf").to_vec())
                .with_context(|| "Failed to load SourceCodePro-Bold.ttf")?;
        let italic =
            Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-It.ttf").to_vec())
                .with_context(|| "Failed to load SourceCodePro-It.ttf")?;
        let bold_italic =
            Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-BoldIt.ttf").to_vec())
                .with_context(|| "Failed to load SourceCodePro-BoldIt.ttf")?;
        Ok(LoadedFonts {
            regular,
            bold,
            italic,
            bold_italic,
        })
    }

    fn load_fira_mono() -> Result<LoadedFonts> {
        let regular =
            Font::load(include_bytes!("../../../assets/fonts/FiraMono-Regular.ttf").to_vec())
                .with_context(|| "Failed to load FiraMono-Regular.ttf")?;
        let bold = Font::load(include_bytes!("../../../assets/fonts/FiraMono-Bold.ttf").to_vec())
            .with_context(|| "Failed to load FiraMono-Bold.ttf")?;
        // FiraMono doesn't have italic variants, reuse regular/bold
        let italic =
            Font::load(include_bytes!("../../../assets/fonts/FiraMono-Regular.ttf").to_vec())
                .with_context(|| "Failed to load FiraMono-Regular.ttf for italic fallback")?;
        let bold_italic =
            Font::load(include_bytes!("../../../assets/fonts/FiraMono-Bold.ttf").to_vec())
                .with_context(|| "Failed to load FiraMono-Bold.ttf for bold-italic fallback")?;
        Ok(LoadedFonts {
            regular,
            bold,
            italic,
            bold_italic,
        })
    }

    fn load_from_path(font_path: &str) -> Result<LoadedFonts> {
        let base = PathBuf::from(font_path);

        // try common naming patterns for font files
        let regular_path = Self::find_font_file(&base, &["Regular", "regular", ""])?;
        let regular_data = std::fs::read(&regular_path)
            .with_context(|| format!("Failed to read font file: {}", regular_path.display()))?;
        let regular = Font::load(regular_data)
            .with_context(|| format!("Failed to parse font file: {}", regular_path.display()))?;

        // for non-regular variants, fall back to regular if not found
        let bold = Self::try_load_variant(&base, &["Bold", "bold"], &regular_path)?;
        let italic =
            Self::try_load_variant(&base, &["Italic", "It", "italic", "it"], &regular_path)?;
        let bold_italic = Self::try_load_variant(
            &base,
            &["BoldItalic", "BoldIt", "bolditalic", "boldit"],
            &regular_path,
        )?;

        Ok(LoadedFonts {
            regular,
            bold,
            italic,
            bold_italic,
        })
    }

    fn find_font_file(base: &Path, suffixes: &[&str]) -> Result<PathBuf> {
        // if base path already has .ttf extension, use it directly
        if base
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
        {
            if base.exists() {
                return Ok(base.to_path_buf());
            }
            return Err(anyhow!("Font file not found: {}", base.display()));
        }

        // try {base}-{suffix}.ttf patterns
        for suffix in suffixes {
            let path = if suffix.is_empty() {
                base.with_extension("ttf")
            } else {
                PathBuf::from(format!("{}-{}.ttf", base.display(), suffix))
            };
            if path.exists() {
                return Ok(path);
            }
        }

        // list what we tried for the error message
        let tried: Vec<String> = suffixes
            .iter()
            .map(|s| {
                if s.is_empty() {
                    format!("{}.ttf", base.display())
                } else {
                    format!("{}-{}.ttf", base.display(), s)
                }
            })
            .collect();

        Err(anyhow!(
            "Could not find font file. Tried: {}. \
            \nHint: Place font files next to src-book.toml with names like MyFont-Regular.ttf, MyFont-Bold.ttf, etc.",
            tried.join(", ")
        ))
    }

    fn try_load_variant(base: &Path, suffixes: &[&str], fallback_path: &Path) -> Result<Font> {
        // try to find the variant file
        for suffix in suffixes {
            let path = PathBuf::from(format!("{}-{}.ttf", base.display(), suffix));
            if path.exists() {
                let data = std::fs::read(&path)
                    .with_context(|| format!("Failed to read font file: {}", path.display()))?;
                return Font::load(data)
                    .with_context(|| format!("Failed to parse font file: {}", path.display()));
            }
        }

        // fall back to regular variant
        let data = std::fs::read(fallback_path).with_context(|| {
            format!("Failed to read fallback font: {}", fallback_path.display())
        })?;
        Font::load(data)
            .with_context(|| format!("Failed to parse fallback font: {}", fallback_path.display()))
    }
}
