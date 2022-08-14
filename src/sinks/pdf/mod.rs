use crate::source::Source;
use anyhow::{Context, Result};
use chrono::TimeZone;
use pdf_gen::layout::Margins;
use pdf_gen::pdf_writer_crate::types::LineCapStyle;
use pdf_gen::pdf_writer_crate::Content;
use pdf_gen::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

#[derive(Debug)]
pub struct PDF {
    outfile: PathBuf,
    ss: SyntaxSet,
    ts: ThemeSet,
}

impl PDF {
    pub fn new<P: AsRef<Path>>(outfile: P) -> PDF {
        let outfile = outfile.as_ref().to_path_buf();
        let ss = bincode::deserialize(crate::highlight::SERIALIZED_SYNTAX)
            .expect("can deserialize syntaxes");
        let ts = ThemeSet::load_defaults();

        PDF { outfile, ss, ts }
    }
}

impl PDF {
    pub fn render(&self, source: &crate::source::Source) -> Result<()> {
        let fira_mono = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/FiraMono-Regular.ttf"
        ));
        let fira_mono = Font::load(fira_mono).with_context(|| "Failed to load Fira Mono font!")?;
        let fira_mono_bold = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/FiraMono-Regular.ttf"
        ));
        let fira_mono_bold =
            Font::load(fira_mono_bold).with_context(|| "Failed to load Fira Mono font!")?;

        let mut doc = Document::default();
        doc.add_font(fira_mono);
        doc.add_font(fira_mono_bold);

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
        doc.add_page(Page::new(pagesize::HALF_LETTER, None));

        let mut source_pages: HashMap<PathBuf, usize> = HashMap::new();
        let mut page_offset = doc.pages.len();
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
                    self.render_image(&mut doc, file)?;
                }
                _ => self
                    .render_source_file(&mut doc, file)
                    .with_context(|| format!("Failed to render source file {}!", file.display()))?,
            }
        }

        page_offset += self
            .render_toc(&mut doc, page_offset, source_pages)
            .with_context(|| "Failed to render table of contents")?;

        // add page numbers
        for (pi, page) in doc.pages.iter_mut().skip(page_offset).enumerate() {
            let text = format!("{}", pi + 1);
            let coords: (Pt, Pt) = if pi % 2 == 0 {
                (
                    page.content_box.x2 - layout::width_of_text(&text, &doc.fonts[0], Pt(8.0)),
                    In(0.25).into(),
                )
            } else {
                (In(0.25).into(), In(0.25).into())
            };
            page.add_span(SpanLayout {
                text,
                font: SpanFont {
                    index: 0,
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

        let page_size = pagesize::HALF_LETTER;
        let descent_title = doc.fonts[1].descent(SIZE_TITLE);

        let title = source.title.clone().unwrap_or("untitled".to_string());
        let authors: Vec<String> = source.authors.iter().map(ToString::to_string).collect();

        let height_title = doc.fonts[1].line_height(SIZE_TITLE);
        let height_by = doc.fonts[0].line_height(SIZE_BY);
        let height_author = doc.fonts[0].line_height(SIZE_AUTHOR);
        let height_total = height_title
            + descent_title
            + height_by
            + (height_author * authors.len() as f32)
            + (SPACING * 2.0);

        let mut page = Page::new(page_size, None);

        let mut y: Pt = (page_size.1 + height_total) / 2.0;

        let x = (page_size.0 - layout::width_of_text(&title, &doc.fonts[1], SIZE_TITLE)) / 2.0;
        page.add_span(SpanLayout {
            text: title,
            font: SpanFont {
                index: 1,
                size: SIZE_TITLE,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_title + SPACING + descent_title;

        let x = (page_size.0 - layout::width_of_text("- by -", &doc.fonts[0], SIZE_BY)) / 2.0;
        page.add_span(SpanLayout {
            text: "- by -".to_string(),
            font: SpanFont {
                index: 1,
                size: SIZE_BY,
            },
            colour: colours::BLACK,
            coords: (x, y),
        });
        y -= height_by + SPACING;

        for author in authors.into_iter() {
            let x =
                (page_size.0 - layout::width_of_text(&author, &doc.fonts[0], SIZE_AUTHOR)) / 2.0;
            page.add_span(SpanLayout {
                text: author,
                font: SpanFont {
                    index: 1,
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
    ) -> Result<usize> {
        const CONTENTS_SIZE: Pt = Pt(24.0);
        const ENTRY_SIZE: Pt = Pt(10.0);

        let height_contents = doc.fonts[1].line_height(CONTENTS_SIZE);
        let height_entry = doc.fonts[0].line_height(ENTRY_SIZE);
        let descent_entry = doc.fonts[0].descent(ENTRY_SIZE);

        let entry_font = SpanFont {
            index: 0,
            size: ENTRY_SIZE,
        };

        let mut entries: Vec<(PathBuf, usize)> = source_pages.into_iter().collect();
        entries.sort_by_key(|(_, p)| *p);

        let mut pages: Vec<Page> = Vec::default();
        while !entries.is_empty() {
            let mut page = Page::new(pagesize::HALF_LETTER, Some(Margins::all(In(0.5).into())));

            let start = if pages.is_empty() {
                layout::baseline_start(&page, &doc.fonts[1], CONTENTS_SIZE)
            } else {
                layout::baseline_start(&page, &doc.fonts[0], ENTRY_SIZE)
            };

            let (x, mut y) = start;
            if pages.is_empty() {
                page.add_span(SpanLayout {
                    text: "Contents".to_string(),
                    font: SpanFont {
                        index: 1,
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
                page.add_span(SpanLayout {
                    text: entry.0.display().to_string(),
                    font: entry_font,
                    colour: colours::BLACK,
                    coords: (x, y),
                });
                let pagenum = format!("{}", entry.1 + 1);
                let pagenum_width = layout::width_of_text(&pagenum, &doc.fonts[1], ENTRY_SIZE);
                page.add_span(SpanLayout {
                    text: pagenum,
                    font: entry_font,
                    colour: colours::BLACK,
                    coords: (page.content_box.x2 - pagenum_width, y),
                });
                y -= height_entry;
            }

            pages.push(page);
        }

        // add a blank page after the contents to keep the booklet even
        if pages.len() % 2 == 1 {
            pages.push(Page::new(pagesize::HALF_LETTER, None));
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

    fn render_image(&self, doc: &mut Document, path: &Path) -> Result<()> {
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
        let mut page = Page::new(pagesize::HALF_LETTER, Some(margins));

        self.render_header(doc, &mut page, path)?;

        let image_size = if aspect_ratio >= 1.0 {
            let width = page.content_box.x2 - page.content_box.x1;
            let height = width / aspect_ratio;
            (width, height)
        } else {
            let height = page.content_box.y2
                - page.content_box.y1
                - doc.fonts[0].line_height(Pt(12.0))
                - In(0.25).into()
                - (doc.fonts[0].line_height(Pt(8.0)) * 2.0);
            let width = height * aspect_ratio;
            (width, height)
        };

        let x =
            (page.content_box.x2 - page.content_box.x1 - image_size.0) / 2.0 + page.content_box.x1;
        let y = (page.content_box.y2 - page.content_box.y1 - image_size.1) / 2.0
            + page.content_box.y1
            + doc.fonts[0].line_height(Pt(8.0));

        page.add_image(ImageLayout {
            image_index,
            position: Rect {
                x1: x,
                y1: y,
                x2: x + image_size.0,
                y2: y + image_size.1,
            },
        });
        let y = y - doc.fonts[0].ascent(Pt(8.0));
        let (file_description, image_description) =
            Self::describe_image(&doc.images[image_index], path);
        page.add_span(SpanLayout {
            text: file_description,
            font: SpanFont {
                index: 0,
                size: Pt(8.0),
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });
        let y = y - doc.fonts[0].line_height(Pt(8.0));
        page.add_span(SpanLayout {
            text: image_description,
            font: SpanFont {
                index: 0,
                size: Pt(8.0),
            },
            colour: Colour::new_grey(0.75),
            coords: (x, y),
        });

        doc.add_page(page);
        Ok(())
    }

    fn render_header(&self, doc: &Document, page: &mut Page, path: &Path) -> Result<()> {
        // add the current file to the top of each page
        // figure out where the header should go
        let header = path.display().to_string();
        let mut header_start = layout::baseline_start(&page, &doc.fonts[0], Pt(12.0));
        let is_even = doc.pages.len() % 2 == 0;
        if is_even {
            header_start.0 =
                page.content_box.x2 - layout::width_of_text(&header, &doc.fonts[0], Pt(12.0));
        }

        // figure out the underline
        let (line_offset, line_thickness) = doc.fonts[0]
            .face
            .underline_metrics()
            .map(|metrics| {
                let scaling = Pt(12.0) / doc.fonts[0].face.units_per_em() as f32;
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
                index: 0,
                size: Pt(12.0),
            },
            colour: Colour::new_grey(0.25),
            coords: header_start,
        });

        Ok(())
    }

    fn render_source_file(&self, doc: &mut Document, path: &Path) -> Result<()> {
        // read the contents
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read contents of {} to string!", path.display()))?;
        let contents = contents.replace("    ", "  ");

        // figure out the syntax if we can
        let syntax = self.ss.find_syntax_by_extension(
            path.extension()
                .map(std::ffi::OsStr::to_str)
                .unwrap_or_default()
                .unwrap_or_default(),
        );

        let text_size: Pt = if path.display().to_string() == "LICENSE" {
            Pt(7.0)
        } else {
            Pt(10.0)
        };

        // start the set of pages with the path
        let mut text: Vec<(String, Colour, SpanFont)> = Vec::default();

        if let Some(syntax) = syntax {
            // load the contents of the file
            let mut h = HighlightLines::new(syntax, &self.ts.themes["InspiredGitHub"]);

            // highlight the file, converting into spans
            for (i, line) in LinesWithEndings::from(contents.as_str()).enumerate() {
                let ranges: Vec<(Style, &str)> =
                    h.highlight_line(line, &self.ss).with_context(|| {
                        format!("Failed to highlight source code for line `{}`", line)
                    })?;

                text.push((
                    format!("{:>4}  ", i + 1),
                    Colour::new_grey(0.75),
                    SpanFont {
                        index: 0,
                        size: Pt(8.0),
                    },
                ));
                for (style, s) in ranges.into_iter() {
                    let colour = Colour::new_rgb_bytes(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    );
                    let index = if style.font_style.intersects(FontStyle::BOLD) {
                        1
                    } else {
                        0
                    };
                    text.push((
                        s.to_string(),
                        colour,
                        SpanFont {
                            index,
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
                        index: 0,
                        size: text_size,
                    },
                ));
            }
        }

        // and render it into pages
        let wrap_width = if syntax.is_some() {
            layout::width_of_text("      ", &doc.fonts[0], Pt(8.0))
        } else {
            Pt(0.0)
        };
        while !text.is_empty() {
            let margins = Margins::trbl(
                In(0.25).into(),
                In(0.25).into(),
                In(0.5).into(),
                In(0.25).into(),
            )
            .with_gutter(In(0.25).into(), doc.pages.len());
            let page_size = pdf_gen::pagesize::HALF_LETTER;

            let mut page = Page::new(page_size, Some(margins));
            let start = layout::baseline_start(&page, &doc.fonts[0], text_size);
            let start = (
                start.0,
                start.1
                    - (doc.fonts[0].ascent(Pt(10.0)) - doc.fonts[0].descent(Pt(12.0)))
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

            self.render_header(doc, &mut page, path)?;
            layout::layout_text(&doc, &mut page, start, &mut text, wrap_width, bbox);
            doc.add_page(page);
        }

        Ok(())
    }
}
