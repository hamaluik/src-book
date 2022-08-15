use crate::source::{Commit, Source};
use anyhow::{Context, Result};
use chrono::TimeZone;
use pdf_gen::layout::Margins;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const PAGE_SIZE: (Pt, Pt) = (Pt(7.0 * 72.0), Pt(8.5 * 72.0));
const FONT_INDEX_REGULAR: usize = 0;
const FONT_INDEX_BOLD: usize = 1;
const FONT_INDEX_ITALIC: usize = 2;
const FONT_INDEX_BOLDITALIC: usize = 3;

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum FontFamily {
    #[serde(rename = "Fira Mono")]
    FiraMono,
    #[serde(rename = "Source Code Pro")]
    SourceCodePro,
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontFamily::FiraMono => write!(f, "Fira Mono"),
            FontFamily::SourceCodePro => write!(f, "Source Code Pro"),
        }
    }
}

impl FontFamily {
    pub fn all() -> &'static [FontFamily] {
        &[FontFamily::FiraMono, FontFamily::SourceCodePro]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDF {
    pub font: FontFamily,
    pub theme: SyntaxTheme,
    pub outfile: PathBuf,
    /*tab_size: usize,
    reduce_spaces: bool,
    native_tab_size: usize,
    page_width_in: f32,
    page_height_in: f32,
    margin_top_in: f32,*/
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_serialize_pdf() {
        let pdf = PDF {
            font: FontFamily::FiraMono,
            theme: SyntaxTheme::SolarizedLight,
            outfile: PathBuf::new(),
        };
        toml::to_string(&pdf).expect("can serialize PDF to TOML");
    }
}

impl PDF {
    pub fn render(&self, source: &crate::source::Source) -> Result<()> {
        let (regular, bold, italic, bold_italic) = match self.font {
            FontFamily::FiraMono => {
                let regular = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/FiraMono-Regular.ttf"
                )))
                .with_context(|| "Failed to load FiraMono-Regular font!")?;
                let bold = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/FiraMono-Bold.ttf"
                )))
                .with_context(|| "Failed to load FiraMono-Bold font!")?;
                let italic = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/FiraMono-Regular.ttf"
                )))
                .with_context(|| "Failed to load FiraMono- font!")?;
                let bold_italic = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/FiraMono-Bold.ttf"
                )))
                .with_context(|| "Failed to load FiraMono-Bold font!")?;
                (regular, bold, italic, bold_italic)
            }
            FontFamily::SourceCodePro => {
                let regular = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/SourceCodePro-Regular.ttf"
                )))
                .with_context(|| "Failed to load SourceCodePro-Regular font!")?;
                let bold = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/SourceCodePro-Bold.ttf"
                )))
                .with_context(|| "Failed to load SourceCodePro-Bold font!")?;
                let italic = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/SourceCodePro-It.ttf"
                )))
                .with_context(|| "Failed to load SourceCodePro-It font!")?;
                let bold_italic = Font::load(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/fonts/SourceCodePro-BoldIt.ttf"
                )))
                .with_context(|| "Failed to load SourceCodePro-BoldIt font!")?;
                (regular, bold, italic, bold_italic)
            }
        };

        let ss: SyntaxSet = bincode::deserialize(crate::highlight::SERIALIZED_SYNTAX)
            .expect("can deserialize syntaxes");
        let ts: ThemeSet = bincode::deserialize(crate::highlight::SERIALIZED_THEMES)
            .expect("can deserialize themes");

        let mut doc = Document::default();
        doc.add_font(regular);
        doc.add_font(bold);
        doc.add_font(italic);
        doc.add_font(bold_italic);

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

        self.render_title_page(&mut doc, source)
            .with_context(|| "Failed to render title page")?;
        // add a blank page after the title page so we start on the right
        doc.add_page(Page::new(PAGE_SIZE, None));

        doc.add_bookmark(None, "Title", 0).borrow_mut().bolded();
        doc.add_bookmark(None, "Table of Contents", 2)
            .borrow_mut()
            .italicized();

        let mut source_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut page_offset = doc.pages.len();

        let source_code_bookmark = doc.add_bookmark(None, "Source Files", doc.pages.len());
        {
            source_code_bookmark.borrow_mut().bolded();
        }

        for file in source.source_files.iter() {
            source_pages.insert(file.clone(), doc.pages.len() - page_offset);

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
                    let page_index = self.render_image(&mut doc, file)?;
                    doc.add_bookmark(
                        Some(source_code_bookmark.clone()),
                        file.display(),
                        page_index,
                    );
                }
                _ => {
                    if let Some(page_index) = self
                        .render_source_file(&mut doc, file, &ss, &ts.themes[self.theme.name()])
                        .with_context(|| {
                            format!("Failed to render source file {}!", file.display())
                        })?
                    {
                        doc.add_bookmark(
                            Some(source_code_bookmark.clone()),
                            file.display(),
                            page_index,
                        );
                    }
                }
            }
        }

        let commits = source
            .commits()
            .with_context(|| "Failed to get commits for repository")?;
        let commit_page_index = self
            .render_commits(&mut doc, commits)
            .with_context(|| "Failed to render commit history")?;
        if let Some(commit_page) = commit_page_index {
            doc.add_bookmark(None, "Commit History", commit_page);
        }

        let num_toc_pages = self
            .render_toc(&mut doc, page_offset, source_pages, commit_page_index)
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
        for (pi, page) in doc.pages.iter_mut().skip(page_offset).enumerate() {
            let text = format!("{}", pi + 1);
            let coords: (Pt, Pt) = if pi % 2 == 0 {
                (
                    page.content_box.x2
                        - layout::width_of_text(&text, &doc.fonts[FONT_INDEX_REGULAR], Pt(8.0)),
                    In(0.25).into(),
                )
            } else {
                (In(0.25).into(), In(0.25).into())
            };
            page.add_span(SpanLayout {
                text,
                font: SpanFont {
                    index: FONT_INDEX_REGULAR,
                    size: Pt(8.0),
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

    fn render_title_page(&self, doc: &mut Document, source: &Source) -> Result<()> {
        const SIZE_TITLE: Pt = Pt(32.0);
        const SIZE_BY: Pt = Pt(8.0);
        const SIZE_AUTHOR: Pt = Pt(10.0);
        const SPACING: Pt = Pt(72.0 * 0.5);

        let page_size = PAGE_SIZE;
        let descent_title = doc.fonts[FONT_INDEX_BOLD].descent(SIZE_TITLE);

        let title = source.title.clone().unwrap_or("untitled".to_string());
        let authors: Vec<String> = source.authors.iter().map(ToString::to_string).collect();

        let height_title = doc.fonts[FONT_INDEX_BOLD].line_height(SIZE_TITLE);
        let height_by = doc.fonts[FONT_INDEX_REGULAR].line_height(SIZE_BY);
        let height_author = doc.fonts[FONT_INDEX_REGULAR].line_height(SIZE_AUTHOR);
        let height_total = height_title
            + descent_title
            + height_by
            + (height_author * authors.len() as f32)
            + (SPACING * 2.0);

        let mut page = Page::new(page_size, None);

        let mut y: Pt = (page_size.1 + height_total) / 2.0;

        let x = (page_size.0
            - layout::width_of_text(&title, &doc.fonts[FONT_INDEX_BOLD], SIZE_TITLE))
            / 2.0;
        page.add_span(SpanLayout {
            text: title,
            font: SpanFont {
                index: FONT_INDEX_BOLD,
                size: SIZE_TITLE,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_title + SPACING + descent_title;

        let x = (page_size.0
            - layout::width_of_text("- by -", &doc.fonts[FONT_INDEX_REGULAR], SIZE_BY))
            / 2.0;
        page.add_span(SpanLayout {
            text: "- by -".to_string(),
            font: SpanFont {
                index: FONT_INDEX_BOLD,
                size: SIZE_BY,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_by + SPACING;

        for author in authors.into_iter() {
            let x = (page_size.0
                - layout::width_of_text(&author, &doc.fonts[FONT_INDEX_REGULAR], SIZE_AUTHOR))
                / 2.0;
            page.add_span(SpanLayout {
                text: author,
                font: SpanFont {
                    index: FONT_INDEX_BOLD,
                    size: SIZE_AUTHOR,
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
        skip_pages: usize,
        source_pages: HashMap<PathBuf, usize>,
        git_history_page: Option<usize>,
    ) -> Result<usize> {
        const CONTENTS_SIZE: Pt = Pt(24.0);
        const ENTRY_SIZE: Pt = Pt(10.0);

        let height_contents = doc.fonts[FONT_INDEX_BOLD].line_height(CONTENTS_SIZE);
        let height_entry = doc.fonts[FONT_INDEX_REGULAR].line_height(ENTRY_SIZE);
        let descent_entry = doc.fonts[FONT_INDEX_REGULAR].descent(ENTRY_SIZE);

        const ENTRY_FONT: SpanFont = SpanFont {
            index: FONT_INDEX_REGULAR,
            size: ENTRY_SIZE,
        };

        // TODO: deal with when we have more than 1 toc page!
        // probably have to pre-calculate how many toc pages we're going to generate
        let mut num_toc_pages = 1;
        if num_toc_pages % 2 == 1 {
            num_toc_pages += 1;
        }

        // figure out the underline
        let (underline_offset, underline_thickness) = doc.fonts[FONT_INDEX_REGULAR]
            .face
            .underline_metrics()
            .map(|metrics| {
                let scaling = Pt(12.0) / doc.fonts[FONT_INDEX_REGULAR].face.units_per_em() as f32;
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
            let mut page = Page::new(PAGE_SIZE, Some(Margins::all(In(0.5).into())));

            let start = if pages.is_empty() {
                layout::baseline_start(&page, &doc.fonts[FONT_INDEX_BOLD], CONTENTS_SIZE)
            } else {
                layout::baseline_start(&page, &doc.fonts[FONT_INDEX_REGULAR], ENTRY_SIZE)
            };

            let (x, mut y) = start;
            if pages.is_empty() {
                page.add_span(SpanLayout {
                    text: "Contents".to_string(),
                    font: SpanFont {
                        index: FONT_INDEX_BOLD,
                        size: CONTENTS_SIZE,
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
                    &doc.fonts[ENTRY_FONT.index],
                    ENTRY_SIZE,
                );
                let pagenum = format!("{}", entry.1 + 1); // page numbering is 0-indexed, add 1 to make it 1-indexed
                let pagenum_width =
                    layout::width_of_text(&pagenum, &doc.fonts[ENTRY_FONT.index], ENTRY_SIZE);

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
                                &doc.fonts[ENTRY_FONT.index],
                                ENTRY_SIZE,
                            ),
                        *y + *underline_offset,
                    )
                    .stroke();
                page.add_content(underline);

                page.add_span(SpanLayout {
                    text: entry.0,
                    font: ENTRY_FONT,
                    colour: colours::BLACK,
                    coords: (x, y),
                });
                page.add_span(SpanLayout {
                    text: pagenum,
                    font: ENTRY_FONT,
                    colour: colours::BLACK,
                    coords: (page.content_box.x2 - pagenum_width, y),
                });

                page.add_intradocument_link(
                    Rect {
                        x1: page.content_box.x1,
                        x2: page.content_box.x2,
                        y1: y,
                        y2: y + doc.fonts[ENTRY_FONT.index].ascent(ENTRY_SIZE),
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
        doc.pages.splice(skip_pages..skip_pages, pages);

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

    fn render_image(&self, doc: &mut Document, path: &Path) -> Result<usize> {
        let image = Image::new_from_disk(path)?;
        let aspect_ratio = image.aspect_ratio();
        let image_index = doc.add_image(image);

        let margins = Margins::trbl(
            In(0.25).into(),
            In(0.25).into(),
            In(0.5).into(),
            In(0.25).into(),
        )
        .with_gutter(In(0.25).into(), doc.pages.len());
        let mut page = Page::new(PAGE_SIZE, Some(margins));

        self.render_header(doc, &mut page, path.display())?;

        let image_size = if aspect_ratio >= 1.0 {
            let width = page.content_box.x2 - page.content_box.x1;
            let height = width / aspect_ratio;
            (width, height)
        } else {
            let height = page.content_box.y2
                - page.content_box.y1
                - doc.fonts[FONT_INDEX_REGULAR].line_height(Pt(12.0))
                - In(0.25).into()
                - (doc.fonts[FONT_INDEX_REGULAR].line_height(Pt(8.0)) * 2.0);
            let width = height * aspect_ratio;
            (width, height)
        };

        let x =
            (page.content_box.x2 - page.content_box.x1 - image_size.0) / 2.0 + page.content_box.x1;
        let y = (page.content_box.y2 - page.content_box.y1 - image_size.1) / 2.0
            + page.content_box.y1
            + doc.fonts[FONT_INDEX_REGULAR].line_height(Pt(8.0));

        page.add_image(ImageLayout {
            image_index,
            position: Rect {
                x1: x,
                y1: y,
                x2: x + image_size.0,
                y2: y + image_size.1,
            },
        });
        let y = y - doc.fonts[FONT_INDEX_REGULAR].ascent(Pt(8.0));
        let (file_description, image_description) =
            Self::describe_image(&doc.images[image_index], path);
        page.add_span(SpanLayout {
            text: file_description,
            font: SpanFont {
                index: FONT_INDEX_REGULAR,
                size: Pt(8.0),
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });
        let y = y - doc.fonts[FONT_INDEX_REGULAR].line_height(Pt(8.0));
        page.add_span(SpanLayout {
            text: image_description,
            font: SpanFont {
                index: FONT_INDEX_REGULAR,
                size: Pt(8.0),
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });

        let page_index = doc.add_page(page);
        Ok(page_index)
    }

    fn render_header<S: ToString>(&self, doc: &Document, page: &mut Page, text: S) -> Result<()> {
        // add the current file to the top of each page
        // figure out where the header should go
        let header = text.to_string();
        let mut header_start =
            layout::baseline_start(&page, &doc.fonts[FONT_INDEX_REGULAR], Pt(12.0));
        let is_even = doc.pages.len() % 2 == 0;
        if is_even {
            header_start.0 = page.content_box.x2
                - layout::width_of_text(&header, &doc.fonts[FONT_INDEX_REGULAR], Pt(12.0));
        }

        // figure out the underline
        let (line_offset, line_thickness) = doc.fonts[FONT_INDEX_REGULAR]
            .face
            .underline_metrics()
            .map(|metrics| {
                let scaling = Pt(12.0) / doc.fonts[FONT_INDEX_REGULAR].face.units_per_em() as f32;
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
                index: FONT_INDEX_REGULAR,
                size: Pt(12.0),
            },
            colour: Colour::new_grey(0.25),
            coords: header_start,
        });

        Ok(())
    }

    fn render_source_file(
        &self,
        doc: &mut Document,
        path: &Path,
        ss: &SyntaxSet,
        theme: &syntect::highlighting::Theme,
    ) -> Result<Option<usize>> {
        // read the contents
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read contents of {} to string!", path.display()))?;
        let contents = contents.replace("    ", "  ");

        // figure out the syntax if we can
        let syntax = ss.find_syntax_by_extension(
            path.extension()
                .map(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .unwrap_or_default(),
        );

        const TEXT_SIZE: Pt = Pt(10.0);

        // start the set of pages with the path
        let mut text: Vec<(String, Colour, SpanFont)> = Vec::default();

        if let Some(syntax) = syntax {
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
                        index: FONT_INDEX_REGULAR,
                        size: Pt(8.0),
                    },
                ));
                for (style, s) in ranges.into_iter() {
                    let colour = Colour::new_rgb_bytes(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    );

                    let index = match (
                        style.font_style.intersects(FontStyle::BOLD),
                        style.font_style.intersects(FontStyle::ITALIC),
                    ) {
                        (true, true) => FONT_INDEX_BOLDITALIC,
                        (true, false) => FONT_INDEX_BOLD,
                        (false, true) => FONT_INDEX_ITALIC,
                        (false, false) => FONT_INDEX_REGULAR,
                    };

                    text.push((
                        s.to_string(),
                        colour,
                        SpanFont {
                            index,
                            size: TEXT_SIZE,
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
                        index: FONT_INDEX_REGULAR,
                        size: TEXT_SIZE,
                    },
                ));
            }
        }

        // and render it into pages
        let wrap_width = if syntax.is_some() {
            layout::width_of_text("      ", &doc.fonts[FONT_INDEX_REGULAR], Pt(8.0))
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
            .with_gutter(In(0.25).into(), doc.pages.len());
            let page_size = PAGE_SIZE;

            let mut page = Page::new(page_size, Some(margins));
            let start = layout::baseline_start(&page, &doc.fonts[FONT_INDEX_REGULAR], TEXT_SIZE);
            let start = (
                start.0,
                start.1
                    - (doc.fonts[FONT_INDEX_REGULAR].ascent(Pt(10.0))
                        - doc.fonts[FONT_INDEX_REGULAR].descent(Pt(12.0)))
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

            self.render_header(doc, &mut page, path.display())?;
            layout::layout_text(&doc, &mut page, start, &mut text, wrap_width, bbox);
            let page_index = doc.add_page(page);
            if first_page.is_none() {
                first_page = Some(page_index);
            }
        }

        Ok(first_page)
    }

    fn render_commits(&self, doc: &mut Document, commits: Vec<Commit>) -> Result<Option<usize>> {
        // convert the commits to a series of text spans
        let mut text: Vec<(String, Colour, SpanFont)> = Vec::with_capacity(commits.len() * 6);

        const SPAN_FONT_NORMAL: SpanFont = SpanFont {
            index: FONT_INDEX_REGULAR,
            size: Pt(8.0),
        };
        const SPAN_FONT_BOLD: SpanFont = SpanFont {
            index: FONT_INDEX_BOLD,
            size: Pt(8.0),
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
                SPAN_FONT_BOLD,
            ));
            if let Some(summary) = summary {
                text.push((
                    format!(" {}\n", summary),
                    Colour::new_rgb_bytes(40, 40, 40),
                    SPAN_FONT_NORMAL,
                ));
            }
            text.push((
                format!("         {}\n", date.to_rfc2822()),
                Colour::new_rgb_bytes(121, 116, 14),
                SPAN_FONT_NORMAL,
            ));
            text.push((
                format!("         {}\n", author),
                Colour::new_rgb_bytes(7, 102, 120),
                SPAN_FONT_NORMAL,
            ));
            if let Some(body) = body {
                text.push((
                    format!("         {}\n", body),
                    Colour::new_rgb_bytes(60, 56, 54),
                    SPAN_FONT_NORMAL,
                ));
            }
            text.push(("\n".to_string(), colours::WHITE, SPAN_FONT_NORMAL));
        }

        // and render it into pages
        let wrap_width = layout::width_of_text(
            "         ",
            &doc.fonts[SPAN_FONT_BOLD.index],
            SPAN_FONT_BOLD.size,
        );
        let mut first_page = None;
        while !text.is_empty() {
            let margins = Margins::trbl(
                In(0.25).into(),
                In(0.25).into(),
                In(0.5).into(),
                In(0.25).into(),
            )
            .with_gutter(In(0.25).into(), doc.pages.len());
            let page_size = PAGE_SIZE;

            // insert a blank page so we open to the correct side
            if first_page.is_none() && doc.pages.len() % 2 == 1 {
                doc.add_page(Page::new(page_size, Some(margins.clone())));
            }

            let mut page = Page::new(page_size, Some(margins));
            let start = layout::baseline_start(
                &page,
                &doc.fonts[SPAN_FONT_BOLD.index],
                SPAN_FONT_BOLD.size,
            );
            let start = (
                start.0,
                start.1
                    - (doc.fonts[SPAN_FONT_BOLD.index].ascent(SPAN_FONT_BOLD.size)
                        - doc.fonts[FONT_INDEX_REGULAR].descent(Pt(12.0)))
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

            self.render_header(doc, &mut page, "Commit History")?;
            layout::layout_text(&doc, &mut page, start, &mut text, wrap_width, bbox);
            let page_index = doc.add_page(page);
            if first_page.is_none() {
                first_page = Some(page_index);
            }
        }

        Ok(first_page)
    }
}
