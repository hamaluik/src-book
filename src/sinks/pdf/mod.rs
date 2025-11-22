use crate::source::{Commit, Source};
use anyhow::{anyhow, Context, Result};
use chrono::TimeZone;
use owned_ttf_parser::AsFaceRef;
use pdf_gen::id_arena_crate::Id;
use pdf_gen::layout::Margins;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const PAGE_SIZE: (Pt, Pt) = (Pt(5.5 * 72.0), Pt(8.5 * 72.0));

/// Font IDs for the document, populated during render.
///
/// Syntax highlighting requires all four variants to properly render bold, italic, and
/// bold-italic code tokens. Fonts that lack certain variants (like FiraMono which has
/// no italic) fall back to regular/bold as appropriate.
struct FontIds {
    regular: Id<Font>,
    bold: Id<Font>,
    italic: Id<Font>,
    bold_italic: Id<Font>,
}

/// Loaded font data before being added to the document.
///
/// Supports three loading modes:
/// - "SourceCodePro": bundled font with full variant support
/// - "FiraMono": bundled font with Regular/Bold only (italic falls back)
/// - "./path/to/Font": custom font loaded from disk using naming conventions
struct LoadedFonts {
    regular: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
}

impl LoadedFonts {
    /// Load fonts based on font name configuration.
    ///
    /// Supports:
    /// - "SourceCodePro" - bundled font with all 4 variants
    /// - "FiraMono" - bundled font (Regular/Bold only, falls back for italic)
    /// - Path like "./fonts/MyFont" - loads MyFont-Regular.ttf, MyFont-Bold.ttf, etc.
    fn load(font_name: &str) -> Result<LoadedFonts> {
        match font_name {
            "SourceCodePro" => Self::load_source_code_pro(),
            "FiraMono" => Self::load_fira_mono(),
            _ => Self::load_from_path(font_name),
        }
    }

    fn load_source_code_pro() -> Result<LoadedFonts> {
        let regular = Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-Regular.ttf").to_vec())
            .with_context(|| "Failed to load SourceCodePro-Regular.ttf")?;
        let bold = Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-Bold.ttf").to_vec())
            .with_context(|| "Failed to load SourceCodePro-Bold.ttf")?;
        let italic = Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-It.ttf").to_vec())
            .with_context(|| "Failed to load SourceCodePro-It.ttf")?;
        let bold_italic = Font::load(include_bytes!("../../../assets/fonts/SourceCodePro-BoldIt.ttf").to_vec())
            .with_context(|| "Failed to load SourceCodePro-BoldIt.ttf")?;
        Ok(LoadedFonts { regular, bold, italic, bold_italic })
    }

    fn load_fira_mono() -> Result<LoadedFonts> {
        let regular = Font::load(include_bytes!("../../../assets/fonts/FiraMono-Regular.ttf").to_vec())
            .with_context(|| "Failed to load FiraMono-Regular.ttf")?;
        let bold = Font::load(include_bytes!("../../../assets/fonts/FiraMono-Bold.ttf").to_vec())
            .with_context(|| "Failed to load FiraMono-Bold.ttf")?;
        // FiraMono doesn't have italic variants, reuse regular/bold
        let italic = Font::load(include_bytes!("../../../assets/fonts/FiraMono-Regular.ttf").to_vec())
            .with_context(|| "Failed to load FiraMono-Regular.ttf for italic fallback")?;
        let bold_italic = Font::load(include_bytes!("../../../assets/fonts/FiraMono-Bold.ttf").to_vec())
            .with_context(|| "Failed to load FiraMono-Bold.ttf for bold-italic fallback")?;
        Ok(LoadedFonts { regular, bold, italic, bold_italic })
    }

    fn load_from_path(font_path: &str) -> Result<LoadedFonts> {
        let base = PathBuf::from(font_path);

        // Try common naming patterns for font files
        let regular_path = Self::find_font_file(&base, &["Regular", "regular", ""])?;
        let regular_data = std::fs::read(&regular_path)
            .with_context(|| format!("Failed to read font file: {}", regular_path.display()))?;
        let regular = Font::load(regular_data)
            .with_context(|| format!("Failed to parse font file: {}", regular_path.display()))?;

        // For non-regular variants, fall back to regular if not found
        let bold = Self::try_load_variant(&base, &["Bold", "bold"], &regular_path)?;
        let italic = Self::try_load_variant(&base, &["Italic", "It", "italic", "it"], &regular_path)?;
        let bold_italic = Self::try_load_variant(&base, &["BoldItalic", "BoldIt", "bolditalic", "boldit"], &regular_path)?;

        Ok(LoadedFonts { regular, bold, italic, bold_italic })
    }

    fn find_font_file(base: &Path, suffixes: &[&str]) -> Result<PathBuf> {
        // If base path already has .ttf extension, use it directly
        if base.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf")) {
            if base.exists() {
                return Ok(base.to_path_buf());
            }
            return Err(anyhow!("Font file not found: {}", base.display()));
        }

        // Try {base}-{suffix}.ttf patterns
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

        // List what we tried for the error message
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
        // Try to find the variant file
        for suffix in suffixes {
            let path = PathBuf::from(format!("{}-{}.ttf", base.display(), suffix));
            if path.exists() {
                let data = std::fs::read(&path)
                    .with_context(|| format!("Failed to read font file: {}", path.display()))?;
                return Font::load(data)
                    .with_context(|| format!("Failed to parse font file: {}", path.display()));
            }
        }

        // Fall back to regular variant
        let data = std::fs::read(fallback_path)
            .with_context(|| format!("Failed to read fallback font: {}", fallback_path.display()))?;
        Font::load(data)
            .with_context(|| format!("Failed to parse fallback font: {}", fallback_path.display()))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum SyntaxTheme {
    #[serde(rename = "Solarized (light)")]
    SolarizedLight,
    #[serde(rename = "OneHalfLight")]
    OneHalfLight,
    #[serde(rename = "gruvbox (Light) (Hard)")]
    Gruvbox,
    #[serde(rename = "GitHub")]
    GitHub,
}

impl fmt::Display for SyntaxTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl SyntaxTheme {
    fn name(&self) -> &'static str {
        match self {
            SyntaxTheme::SolarizedLight => "Solarized (light)",
            SyntaxTheme::OneHalfLight => "OneHalfLight",
            SyntaxTheme::Gruvbox => "gruvbox (Light) (Hard)",
            SyntaxTheme::GitHub => "GitHub",
        }
    }

    pub fn all() -> &'static [SyntaxTheme] {
        &[
            SyntaxTheme::SolarizedLight,
            SyntaxTheme::OneHalfLight,
            SyntaxTheme::Gruvbox,
            SyntaxTheme::GitHub,
        ]
    }
}

/// PDF output configuration.
///
/// Margins are asymmetric to support booklet printing: inner margins accommodate
/// binding, while outer margins can be smaller. Top margins are typically larger
/// than bottom to leave room for headers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDF {
    /// Font family name ("SourceCodePro", "FiraMono") or path to custom font
    pub font: String,
    /// Syntax highlighting theme for code blocks
    pub theme: SyntaxTheme,
    /// Output PDF file path
    pub outfile: PathBuf,
    /// Page width in inches
    pub page_width_in: f32,
    /// Page height in inches
    pub page_height_in: f32,
    /// Top margin in inches (typically larger for headers)
    pub margin_top_in: f32,
    /// Outer margin in inches (away from binding)
    pub margin_outer_in: f32,
    /// Bottom margin in inches
    pub margin_bottom_in: f32,
    /// Inner margin in inches (binding/gutter side)
    pub margin_inner_in: f32,
    #[serde(default = "default_font_size_title")]
    pub font_size_title_pt: f32,
    #[serde(default = "default_font_size_heading")]
    pub font_size_heading_pt: f32,
    #[serde(default = "default_font_size_subheading")]
    pub font_size_subheading_pt: f32,
    #[serde(default = "default_font_size_body")]
    pub font_size_body_pt: f32,
    #[serde(default = "default_font_size_small")]
    pub font_size_small_pt: f32,
}

fn default_font_size_title() -> f32 { 32.0 }
fn default_font_size_heading() -> f32 { 24.0 }
fn default_font_size_subheading() -> f32 { 12.0 }
fn default_font_size_body() -> f32 { 10.0 }
fn default_font_size_small() -> f32 { 8.0 }

impl Default for PDF {
    fn default() -> Self {
        PDF {
            font: "SourceCodePro".to_string(),
            theme: SyntaxTheme::GitHub,
            outfile: PathBuf::from("book.pdf"),
            page_width_in: 5.5,
            page_height_in: 8.5,
            margin_top_in: 0.5,
            margin_outer_in: 0.125,
            margin_bottom_in: 0.25,
            margin_inner_in: 0.25,
            font_size_title_pt: default_font_size_title(),
            font_size_heading_pt: default_font_size_heading(),
            font_size_subheading_pt: default_font_size_subheading(),
            font_size_body_pt: default_font_size_body(),
            font_size_small_pt: default_font_size_small(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_serialize_pdf() {
        let pdf = PDF::default();
        toml::to_string(&pdf).expect("can serialize PDF to TOML");
    }
}

impl PDF {
    pub fn render(&self, source: &crate::source::Source) -> Result<()> {
        // Load fonts based on configuration
        let fonts = LoadedFonts::load(&self.font)
            .with_context(|| format!("Failed to load font '{}'", self.font))?;

        let ss: SyntaxSet = bincode::deserialize(crate::highlight::SERIALIZED_SYNTAX)
            .expect("can deserialize syntaxes");
        let ts: ThemeSet = bincode::deserialize(crate::highlight::SERIALIZED_THEMES)
            .expect("can deserialize themes");

        let mut doc = Document::default();
        let font_ids = FontIds {
            regular: doc.add_font(fonts.regular),
            bold: doc.add_font(fonts.bold),
            italic: doc.add_font(fonts.italic),
            bold_italic: doc.add_font(fonts.bold_italic),
        };

        let mut info = Info::default();
        if let Some(title) = &source.title {
            info.title(title);
        }
        let authors = source
            .authors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(" ");
        if !authors.trim().is_empty() {
            info.author(authors);
        }

        self.render_title_page(&mut doc, &font_ids, source)
            .with_context(|| "Failed to render title page")?;
        // add a blank page after the title page so we start on the right
        doc.add_page(Page::new(PAGE_SIZE, None));

        doc.add_bookmark(None, "Title", 0).borrow_mut().bolded();
        doc.add_bookmark(None, "Table of Contents", 2)
            .borrow_mut()
            .italicized();

        let mut source_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut page_offset = doc.page_order.len();

        let source_code_bookmark = doc.add_bookmark(None, "Source Files", doc.page_order.len());
        {
            source_code_bookmark.borrow_mut().bolded();
        }

        // track folder bookmarks for hierarchical structure
        let mut folder_bookmarks: HashMap<PathBuf, Rc<RefCell<OutlineEntry>>> = HashMap::new();

        /// get or create folder bookmarks for all ancestor directories of a file path,
        /// returning the immediate parent folder's bookmark
        fn get_or_create_folder_bookmark(
            doc: &mut Document,
            folder_bookmarks: &mut HashMap<PathBuf, Rc<RefCell<OutlineEntry>>>,
            root_bookmark: &Rc<RefCell<OutlineEntry>>,
            file_path: &Path,
            page_index: usize,
        ) -> Rc<RefCell<OutlineEntry>> {
            let parent = match file_path.parent() {
                Some(p) if !p.as_os_str().is_empty() => p,
                _ => return root_bookmark.clone(),
            };

            // collect all ancestor paths that need bookmarks
            let mut ancestors: Vec<&Path> = Vec::new();
            let mut current = parent;
            while !current.as_os_str().is_empty() {
                if !folder_bookmarks.contains_key(current) {
                    ancestors.push(current);
                }
                current = match current.parent() {
                    Some(p) => p,
                    None => break,
                };
            }

            // create bookmarks from root to leaf (reverse order)
            for ancestor in ancestors.into_iter().rev() {
                let parent_bookmark = match ancestor.parent() {
                    Some(p) if !p.as_os_str().is_empty() => {
                        folder_bookmarks.get(p).cloned().unwrap_or_else(|| root_bookmark.clone())
                    }
                    _ => root_bookmark.clone(),
                };

                // use just the folder name with trailing slash for display
                let folder_name = ancestor
                    .file_name()
                    .map(|n| format!("{}/", n.to_string_lossy()))
                    .unwrap_or_else(|| format!("{}/", ancestor.display()));

                let bookmark = doc.add_bookmark(Some(parent_bookmark), folder_name, page_index);
                folder_bookmarks.insert(ancestor.to_path_buf(), bookmark);
            }

            folder_bookmarks.get(parent).cloned().unwrap_or_else(|| root_bookmark.clone())
        }

        for file in source.source_files.iter() {
            source_pages.insert(file.clone(), doc.page_order.len() - page_offset);

            // render an image or source file depending on its extension
            match file
                .extension()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .to_str()
                .unwrap_or_default()
            {
                "png" | "svg" | "bmp" | "ico" | "jpg" | "jpeg" | "webp" | "avif" | "tga"
                | "tiff" => {
                    let page_index = self.render_image(&mut doc, &font_ids, file)?;
                    let parent_bookmark = get_or_create_folder_bookmark(
                        &mut doc,
                        &mut folder_bookmarks,
                        &source_code_bookmark,
                        file,
                        page_index,
                    );
                    let file_name = file.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.display().to_string());
                    doc.add_bookmark(Some(parent_bookmark), file_name, page_index);
                }
                _ => {
                    if let Some(page_index) = self
                        .render_source_file(&mut doc, &font_ids, file, &ss, &ts.themes[self.theme.name()])
                        .with_context(|| {
                            format!("Failed to render source file {}!", file.display())
                        })?
                    {
                        let parent_bookmark = get_or_create_folder_bookmark(
                            &mut doc,
                            &mut folder_bookmarks,
                            &source_code_bookmark,
                            file,
                            page_index,
                        );
                        let file_name = file.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.display().to_string());
                        doc.add_bookmark(Some(parent_bookmark), file_name, page_index);
                    }
                }
            }
        }

        let commits = source
            .commits()
            .with_context(|| "Failed to get commits for repository")?;
        let commit_page_index = self
            .render_commits(&mut doc, &font_ids, commits)
            .with_context(|| "Failed to render commit history")?;
        if let Some(commit_page) = commit_page_index {
            doc.add_bookmark(None, "Commit History", commit_page);
        }

        let num_toc_pages = self
            .render_toc(&mut doc, &font_ids, page_offset, source_pages, commit_page_index)
            .with_context(|| "Failed to render table of contents")?;
        page_offset += num_toc_pages;

        // adjust the page numbering of all our source file bookmarks because we inserted a TOC ahead of them
        fn offset_bookmark_page_indices(
            items: &mut [std::rc::Rc<std::cell::RefCell<pdf_gen::OutlineEntry>>],
            offset_amount: usize,
        ) {
            for item in items {
                let has_children = !item.borrow().children.is_empty();
                if has_children {
                    offset_bookmark_page_indices(&mut item.borrow_mut().children, offset_amount)
                }
                item.borrow_mut().page_index += offset_amount;
            }
        }
        for entry in doc.outline.entries.iter_mut().skip(2) {
            entry.borrow_mut().page_index += num_toc_pages;
            if !entry.borrow().children.is_empty() {
                offset_bookmark_page_indices(&mut entry.borrow_mut().children, num_toc_pages);
            }
        }

        // add page numbers
        let page_number_size = Pt(self.font_size_small_pt);
        for (pi, page_id) in doc.page_order.iter().skip(page_offset).enumerate() {
            let text = format!("{}", pi + 1);
            let page = doc.pages.get_mut(*page_id).expect("page exists");
            let coords: (Pt, Pt) = if pi % 2 == 0 {
                (
                    page.content_box.x2
                        - layout::width_of_text(&text, &doc.fonts[font_ids.regular], page_number_size),
                    In(0.25).into(),
                )
            } else {
                (In(0.25).into(), In(0.25).into())
            };
            page.add_span(SpanLayout {
                text,
                font: SpanFont {
                    id: font_ids.regular,
                    size: page_number_size,
                },
                colour: Colour::new_grey(0.25),
                coords,
            });
        }

        let file =
            std::fs::File::create(&self.outfile).with_context(|| "Failed to create output file")?;
        let mut file = std::io::BufWriter::new(file);
        doc.write(&mut file)
            .with_context(|| "Failed to render PDF")?;

        Ok(())
    }

    fn render_title_page(&self, doc: &mut Document, font_ids: &FontIds, source: &Source) -> Result<()> {
        let size_title = Pt(self.font_size_title_pt);
        let size_by = Pt(self.font_size_small_pt);
        let size_author = Pt(self.font_size_body_pt);
        const SPACING: Pt = Pt(72.0 * 0.5);

        let page_size = PAGE_SIZE;
        let descent_title = doc.fonts[font_ids.bold].descent(size_title);

        let title = source.title.clone().unwrap_or("untitled".to_string());
        let mut authors = source.authors.clone();
        authors.sort();
        let authors: Vec<String> = authors.iter().map(ToString::to_string).collect();

        let height_title = doc.fonts[font_ids.bold].line_height(size_title);
        let height_by = doc.fonts[font_ids.regular].line_height(size_by);
        let height_author = doc.fonts[font_ids.regular].line_height(size_author);
        let height_total = height_title
            + descent_title
            + height_by
            + (height_author * authors.len() as f32)
            + (SPACING * 2.0);

        let mut page = Page::new(page_size, None);

        let mut y: Pt = (page_size.1 + height_total) / 2.0;

        let x = (page_size.0
            - layout::width_of_text(&title, &doc.fonts[font_ids.bold], size_title))
            / 2.0;
        page.add_span(SpanLayout {
            text: title,
            font: SpanFont {
                id: font_ids.bold,
                size: size_title,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_title + SPACING + descent_title;

        let x = (page_size.0
            - layout::width_of_text("- by -", &doc.fonts[font_ids.regular], size_by))
            / 2.0;
        page.add_span(SpanLayout {
            text: "- by -".to_string(),
            font: SpanFont {
                id: font_ids.bold,
                size: size_by,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_by + SPACING;

        for author in authors.into_iter() {
            let x = (page_size.0
                - layout::width_of_text(&author, &doc.fonts[font_ids.regular], size_author))
                / 2.0;
            page.add_span(SpanLayout {
                text: author,
                font: SpanFont {
                    id: font_ids.bold,
                    size: size_author,
                },
                colour: colours::BLACK,
                coords: (x, y),
            });
            y -= height_author;
        }

        doc.add_page(page);
        Ok(())
    }

    fn render_toc(
        &self,
        doc: &mut Document,
        font_ids: &FontIds,
        skip_pages: usize,
        source_pages: HashMap<PathBuf, usize>,
        git_history_page: Option<usize>,
    ) -> Result<usize> {
        let contents_size = Pt(self.font_size_heading_pt);
        let entry_size = Pt(self.font_size_body_pt);
        let subheading_size = Pt(self.font_size_subheading_pt);

        let height_contents = doc.fonts[font_ids.bold].line_height(contents_size);
        let height_entry = doc.fonts[font_ids.regular].line_height(entry_size);
        let descent_entry = doc.fonts[font_ids.regular].descent(entry_size);

        let entry_font = SpanFont {
            id: font_ids.regular,
            size: entry_size,
        };

        // TODO: deal with when we have more than 1 toc page!
        // probably have to pre-calculate how many toc pages we're going to generate
        let mut num_toc_pages = 1;
        if num_toc_pages % 2 == 1 {
            num_toc_pages += 1;
        }

        // figure out the underline
        let (underline_offset, underline_thickness) = doc.fonts[font_ids.regular]
            .face
            .as_face_ref()
            .underline_metrics()
            .map(|metrics| {
                let scaling = subheading_size / doc.fonts[font_ids.regular].face.as_face_ref().units_per_em() as f32;
                (
                    scaling * metrics.position as f32,
                    scaling * metrics.thickness as f32,
                )
            })
            .unwrap_or_else(|| (Pt(-2.0), Pt(0.5)));

        let mut entries: Vec<(String, usize)> = source_pages
            .into_iter()
            .map(|(path, pi)| (path.display().to_string(), pi))
            .collect();
        if let Some(git_history_page) = git_history_page {
            entries.push(("Commit History".to_string(), git_history_page - skip_pages));
        }
        entries.sort_by_key(|(_, p)| *p);

        let mut pages: Vec<Page> = Vec::default();
        while !entries.is_empty() {
            let mut page = Page::new(PAGE_SIZE, Some(Margins::all(In(0.5))));

            let start = if pages.is_empty() {
                layout::baseline_start(&page, &doc.fonts[font_ids.bold], contents_size)
            } else {
                layout::baseline_start(&page, &doc.fonts[font_ids.regular], entry_size)
            };

            let (x, mut y) = start;
            if pages.is_empty() {
                page.add_span(SpanLayout {
                    text: "Contents".to_string(),
                    font: SpanFont {
                        id: font_ids.bold,
                        size: contents_size,
                    },
                    colour: colours::BLACK,
                    coords: (x, y),
                });
                y -= height_contents;
            }

            'page: loop {
                if y < page.content_box.y1 + descent_entry || entries.is_empty() {
                    break 'page;
                }

                let entry = entries.remove(0);
                let entry_width = layout::width_of_text(
                    &format!("{} ", entry.0),
                    &doc.fonts[font_ids.regular],
                    entry_size,
                );
                let pagenum = format!("{}", entry.1 + 1); // page numbering is 0-indexed, add 1 to make it 1-indexed
                let pagenum_width =
                    layout::width_of_text(&pagenum, &doc.fonts[font_ids.regular], entry_size);

                let mut underline = Content::new();
                underline
                    .set_stroke_gray(0.75)
                    .set_line_cap(LineCapStyle::ButtCap)
                    .set_line_width(*underline_thickness)
                    .move_to(*page.content_box.x1 + *entry_width, *y + *underline_offset)
                    .line_to(
                        *page.content_box.x2
                            - *layout::width_of_text(
                                &format!(" {}", pagenum),
                                &doc.fonts[font_ids.regular],
                                entry_size,
                            ),
                        *y + *underline_offset,
                    )
                    .stroke();
                page.add_content(underline);

                page.add_span(SpanLayout {
                    text: entry.0,
                    font: entry_font,
                    colour: colours::BLACK,
                    coords: (x, y),
                });
                page.add_span(SpanLayout {
                    text: pagenum,
                    font: entry_font,
                    colour: colours::BLACK,
                    coords: (page.content_box.x2 - pagenum_width, y),
                });

                page.add_intradocument_link_by_index(
                    Rect {
                        x1: page.content_box.x1,
                        x2: page.content_box.x2,
                        y1: y,
                        y2: y + doc.fonts[font_ids.regular].ascent(entry_size),
                    },
                    entry.1 + skip_pages + num_toc_pages,
                );

                y -= height_entry;
            }

            pages.push(page);
        }

        // add a blank page after the contents to keep the booklet even
        if pages.len() % 2 == 1 {
            pages.push(Page::new(PAGE_SIZE, None));
        }

        let added_page_count = pages.len();
        // Add pages to the arena and collect their IDs
        let page_ids: Vec<Id<Page>> = pages.into_iter().map(|p| doc.pages.alloc(p)).collect();
        // Insert the IDs into page_order at the correct position
        for (i, page_id) in page_ids.into_iter().enumerate() {
            doc.page_order.insert(skip_pages + i, page_id);
        }

        Ok(added_page_count)
    }

    fn describe_image(image: &Image, path: &Path) -> (String, String) {
        let mut file_description: String = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if let Ok(metadata) = std::fs::metadata(path) {
            let file_size = metadata.len();
            let file_size = byte_unit::Byte::from_bytes(file_size as u128);
            let file_size = file_size.get_appropriate_unit(false).format(2);
            file_description.push_str(", ");
            file_description.push_str(&file_size);

            if let Ok(created) = metadata.created() {
                let unix_time = created
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();

                let created = chrono::Utc.timestamp(unix_time.as_secs() as i64, 0);
                file_description.push_str(&format!(" Created {}", created.to_rfc2822()));
            }
        }

        let mut image_description = String::new();
        match &image.image {
            ImageType::Raster(RasterImageType::DirectlyEmbeddableJpeg(_)) => {
                let w = image.width as usize;
                let h = image.height as usize;
                let format = "rgb8";
                image_description.push_str(&format!("{w}px by {h}px [{format}]"));
            }
            ImageType::Raster(RasterImageType::Image(im)) => {
                let w = image.width as usize;
                let h = image.height as usize;
                let format = match im.color() {
                    pdf_gen::image_crate::ColorType::L8 => "l8",
                    pdf_gen::image_crate::ColorType::La8 => "la8",
                    pdf_gen::image_crate::ColorType::Rgb8 => "rgb8",
                    pdf_gen::image_crate::ColorType::Rgba8 => "rgba8",
                    pdf_gen::image_crate::ColorType::L16 => "l16",
                    pdf_gen::image_crate::ColorType::La16 => "la16",
                    pdf_gen::image_crate::ColorType::Rgb16 => "rgb16",
                    pdf_gen::image_crate::ColorType::Rgba16 => "rgba16",
                    pdf_gen::image_crate::ColorType::Rgb32F => "rgb32f",
                    pdf_gen::image_crate::ColorType::Rgba32F => "rgba32f",
                    _ => "unknown format",
                };
                image_description.push_str(&format!("{w}px by {h}px [{format}]"));
            }
            ImageType::SVG(tree) => {
                let viewbox = tree.svg_node().view_box.rect;
                let x = viewbox.x();
                let y = viewbox.y();
                let w = viewbox.width();
                let h = viewbox.height();
                image_description.push_str(&format!("SVG viewbox: [{x} {y} {w} {h}]"));
            }
        }

        (file_description, image_description)
    }

    fn render_image(&self, doc: &mut Document, font_ids: &FontIds, path: &Path) -> Result<usize> {
        let subheading_size = Pt(self.font_size_subheading_pt);
        let small_size = Pt(self.font_size_small_pt);

        let image = Image::new_from_disk(path)?;
        let aspect_ratio = image.aspect_ratio();
        let image_id = doc.add_image(image);
        let image_index = image_id.index();

        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.page_order.len());
        let mut page = Page::new(PAGE_SIZE, Some(margins));

        self.render_header(doc, font_ids, &mut page, path.display())?;

        let image_size = if aspect_ratio >= 1.0 {
            let width = page.content_box.x2 - page.content_box.x1;
            let height = width / aspect_ratio;
            (width, height)
        } else {
            let height = page.content_box.y2
                - page.content_box.y1
                - doc.fonts[font_ids.regular].line_height(subheading_size)
                - In(0.25).into()
                - (doc.fonts[font_ids.regular].line_height(small_size) * 2.0);
            let width = height * aspect_ratio;
            (width, height)
        };

        let x =
            (page.content_box.x2 - page.content_box.x1 - image_size.0) / 2.0 + page.content_box.x1;
        let y = (page.content_box.y2 - page.content_box.y1 - image_size.1) / 2.0
            + page.content_box.y1
            + doc.fonts[font_ids.regular].line_height(small_size);

        page.add_image(ImageLayout {
            image_index,
            position: Rect {
                x1: x,
                y1: y,
                x2: x + image_size.0,
                y2: y + image_size.1,
            },
        });
        let y = y - doc.fonts[font_ids.regular].ascent(small_size);
        let (file_description, image_description) =
            Self::describe_image(&doc.images[image_id], path);
        page.add_span(SpanLayout {
            text: file_description,
            font: SpanFont {
                id: font_ids.regular,
                size: small_size,
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });
        let y = y - doc.fonts[font_ids.regular].line_height(small_size);
        page.add_span(SpanLayout {
            text: image_description,
            font: SpanFont {
                id: font_ids.regular,
                size: small_size,
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });

        let page_id = doc.add_page(page);
        let page_index = doc.index_of_page(page_id).expect("page was just added");
        Ok(page_index)
    }

    fn render_header<S: ToString>(&self, doc: &Document, font_ids: &FontIds, page: &mut Page, text: S) -> Result<()> {
        let subheading_size = Pt(self.font_size_subheading_pt);

        // add the current file to the top of each page
        // figure out where the header should go
        let header = text.to_string();
        let mut header_start =
            layout::baseline_start(&page, &doc.fonts[font_ids.regular], subheading_size);
        let is_even = doc.page_order.len() % 2 == 0;
        if is_even {
            header_start.0 = page.content_box.x2
                - layout::width_of_text(&header, &doc.fonts[font_ids.regular], subheading_size);
        }

        // figure out the underline
        let (line_offset, line_thickness) = doc.fonts[font_ids.regular]
            .face
            .as_face_ref()
            .underline_metrics()
            .map(|metrics| {
                let scaling = subheading_size / doc.fonts[font_ids.regular].face.as_face_ref().units_per_em() as f32;
                (
                    scaling * metrics.position as f32,
                    scaling * metrics.thickness as f32,
                )
            })
            .unwrap_or_else(|| (Pt(-2.0), Pt(0.5)));

        // add a line below the header
        let mut content = Content::new();
        content
            .set_stroke_gray(0.75)
            .set_line_cap(LineCapStyle::ButtCap)
            .set_line_width(*line_thickness)
            .move_to(*page.content_box.x1, *header_start.1 + *line_offset)
            .line_to(*page.content_box.x2, *header_start.1 + *line_offset)
            .stroke();
        page.add_content(content);

        // write the header
        page.add_span(SpanLayout {
            text: header,
            font: SpanFont {
                id: font_ids.regular,
                size: subheading_size,
            },
            colour: Colour::new_grey(0.25),
            coords: header_start,
        });

        Ok(())
    }

    fn render_source_file(
        &self,
        doc: &mut Document,
        font_ids: &FontIds,
        path: &Path,
        ss: &SyntaxSet,
        theme: &syntect::highlighting::Theme,
    ) -> Result<Option<usize>> {
        let text_size = Pt(self.font_size_body_pt);
        let small_size = Pt(self.font_size_small_pt);
        let subheading_size = Pt(self.font_size_subheading_pt);

        // read the contents, or use placeholder for binary files
        let (contents, is_binary) = match std::fs::read_to_string(path) {
            Ok(contents) => (contents.replace("    ", "  "), false),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                // Binary file - use placeholder
                ("<binary data>".to_string(), true)
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to read contents of {}", path.display()));
            }
        };

        // figure out the syntax if we can (skip for binary files)
        let syntax = if is_binary {
            None
        } else {
            ss.find_syntax_by_extension(
                path.extension()
                    .map(std::ffi::OsStr::to_str)
                    .unwrap_or_default()
                    .unwrap_or_default(),
            )
        };

        // start the set of pages with the path
        let mut text: Vec<(String, Colour, SpanFont)> = Vec::default();

        if is_binary {
            // render binary placeholder
            text.push((
                contents,
                Colour::new_grey(0.5),
                SpanFont {
                    id: font_ids.italic,
                    size: text_size,
                },
            ));
        } else if let Some(syntax) = syntax {
            // load the contents of the file
            let mut h = HighlightLines::new(syntax, theme);

            // highlight the file, converting into spans
            for (i, line) in LinesWithEndings::from(contents.as_str()).enumerate() {
                let ranges: Vec<(Style, &str)> =
                    h.highlight_line(line, &ss).with_context(|| {
                        format!("Failed to highlight source code for line `{}`", line)
                    })?;

                text.push((
                    format!("{:>4}  ", i + 1),
                    Colour::new_grey(0.75),
                    SpanFont {
                        id: font_ids.regular,
                        size: small_size,
                    },
                ));
                for (style, s) in ranges.into_iter() {
                    let colour = Colour::new_rgb_bytes(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    );

                    let font_id = match (
                        style.font_style.intersects(FontStyle::BOLD),
                        style.font_style.intersects(FontStyle::ITALIC),
                    ) {
                        (true, true) => font_ids.bold_italic,
                        (true, false) => font_ids.bold,
                        (false, true) => font_ids.italic,
                        (false, false) => font_ids.regular,
                    };

                    text.push((
                        s.to_string(),
                        colour,
                        SpanFont {
                            id: font_id,
                            size: text_size,
                        },
                    ));
                }
            }
        } else {
            // render without syntax highlighting
            // note: don't show line numbers on these files
            for line in contents.lines() {
                text.push((
                    format!("{}\n", line),
                    colours::BLACK,
                    SpanFont {
                        id: font_ids.regular,
                        size: text_size,
                    },
                ));
            }
        }

        // and render it into pages
        let wrap_width = if syntax.is_some() {
            layout::width_of_text("      ", &doc.fonts[font_ids.regular], small_size)
        } else {
            Pt(0.0)
        };
        let mut first_page = None;
        while !text.is_empty() {
            let margins = Margins::trbl(
                In(0.25).into(),
                In(0.25).into(),
                In(0.5).into(),
                In(0.25).into(),
            )
            .with_gutter(In(0.25).into(), doc.page_order.len());
            let page_size = PAGE_SIZE;

            let mut page = Page::new(page_size, Some(margins));
            let start = layout::baseline_start(&page, &doc.fonts[font_ids.regular], text_size);
            let start = (
                start.0,
                start.1
                    - (doc.fonts[font_ids.regular].ascent(text_size)
                        - doc.fonts[font_ids.regular].descent(subheading_size))
                    - In(0.125).into(),
            );
            let bbox = page.content_box.clone();

            // don't start a page with empty lines
            while let Some(span) = text.first() {
                if span.0 == "\n" {
                    text.remove(0);
                } else {
                    break;
                }
            }
            if text.is_empty() {
                break;
            }

            self.render_header(doc, font_ids, &mut page, path.display())?;
            layout::layout_text_naive(&doc, &mut page, start, &mut text, wrap_width, bbox);
            let page_id = doc.add_page(page);
            if first_page.is_none() {
                first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
            }
        }

        Ok(first_page)
    }

    fn render_commits(&self, doc: &mut Document, font_ids: &FontIds, commits: Vec<Commit>) -> Result<Option<usize>> {
        let small_size = Pt(self.font_size_small_pt);
        let subheading_size = Pt(self.font_size_subheading_pt);

        // convert the commits to a series of text spans
        let mut text: Vec<(String, Colour, SpanFont)> = Vec::with_capacity(commits.len() * 6);

        let span_font_normal = SpanFont {
            id: font_ids.regular,
            size: small_size,
        };
        let span_font_bold = SpanFont {
            id: font_ids.bold,
            size: small_size,
        };

        for commit in commits.into_iter() {
            let Commit {
                author,
                summary,
                body,
                date,
                hash,
            } = commit;

            text.push((
                hash.chars().take(8).collect(),
                Colour::new_rgb_bytes(143, 63, 113),
                span_font_bold,
            ));
            if let Some(summary) = summary {
                text.push((
                    format!(" {}\n", summary),
                    Colour::new_rgb_bytes(40, 40, 40),
                    span_font_normal,
                ));
            }
            text.push((
                format!("         {}\n", date.to_rfc2822()),
                Colour::new_rgb_bytes(121, 116, 14),
                span_font_normal,
            ));
            text.push((
                format!("         {}\n", author),
                Colour::new_rgb_bytes(7, 102, 120),
                span_font_normal,
            ));
            if let Some(body) = body {
                text.push((
                    format!("         {}\n", body),
                    Colour::new_rgb_bytes(60, 56, 54),
                    span_font_normal,
                ));
            }
            text.push(("\n".to_string(), colours::WHITE, span_font_normal));
        }

        // and render it into pages
        let wrap_width = layout::width_of_text(
            "         ",
            &doc.fonts[font_ids.bold],
            span_font_bold.size,
        );
        let mut first_page = None;
        while !text.is_empty() {
            let margins = Margins::trbl(
                In(0.25).into(),
                In(0.25).into(),
                In(0.5).into(),
                In(0.25).into(),
            )
            .with_gutter(In(0.25).into(), doc.page_order.len().saturating_sub(1));
            let page_size = PAGE_SIZE;

            // insert a blank page so we open to the correct side
            if first_page.is_none() && doc.page_order.len() % 2 == 1 {
                doc.add_page(Page::new(page_size, Some(margins.clone())));
            }

            let mut page = Page::new(page_size, Some(margins));
            let start = layout::baseline_start(
                &page,
                &doc.fonts[font_ids.bold],
                span_font_bold.size,
            );
            let start = (
                start.0,
                start.1
                    - (doc.fonts[font_ids.bold].ascent(span_font_bold.size)
                        - doc.fonts[font_ids.regular].descent(subheading_size))
                    - In(0.125).into(),
            );
            let bbox = page.content_box.clone();

            // don't start a page with empty lines
            while let Some(span) = text.first() {
                if span.0 == "\n" {
                    text.remove(0);
                } else {
                    break;
                }
            }
            if text.is_empty() {
                break;
            }

            self.render_header(doc, font_ids, &mut page, "Commit History")?;
            layout::layout_text_naive(&doc, &mut page, start, &mut text, wrap_width, bbox);
            let page_id = doc.add_page(page);
            if first_page.is_none() {
                first_page = Some(doc.index_of_page(page_id).expect("page was just added"));
            }
        }

        Ok(first_page)
    }
}
