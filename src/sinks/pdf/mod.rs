use crate::source::Source;
use anyhow::{Context, Result};
use pdf_gen::layout::Margins;
use pdf_gen::pdf_writer::types::LineCapStyle;
use pdf_gen::pdf_writer::Content;
use pdf_gen::*;
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
        let ss = SyntaxSet::load_defaults_newlines();
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

        for file in source.source_files.iter() {
            self.render_source_file(&mut doc, file)
                .with_context(|| format!("Failed to render source file {}!", file.display()))?;
        }

        // add page numbers
        for (pi, page) in doc.pages.iter_mut().skip(2).enumerate() {
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
                            size: Pt(10.0),
                        },
                    ));
                }
            }
        } else {
            // render without syntax highlighting
            for (i, line) in contents.lines().enumerate() {
                text.push((
                    format!("{:>4}  ", i + 1),
                    Colour::new_grey(0.75),
                    SpanFont {
                        index: 0,
                        size: Pt(8.0),
                    },
                ));

                text.push((
                    format!("{}\n", line),
                    colours::BLACK,
                    SpanFont {
                        index: 0,
                        size: Pt(10.0),
                    },
                ));
            }
        }

        // and render it into pages
        let wrap_width = layout::width_of_text("      ", &doc.fonts[0], Pt(8.0));
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
            let start = layout::baseline_start(&page, &doc.fonts[0], Pt(10.0));
            let start = (
                start.0,
                start.1 - (doc.fonts[0].line_height(Pt(12.0)) * 2.0),
            );
            let bbox = page.content_box.clone();

            // add the current file to the top of each page
            // figure out where the header should go
            let header = path.display().to_string();
            let mut header_start = layout::baseline_start(&page, &doc.fonts[0], Pt(12.0));
            let is_even = doc.pages.len() % 2 == 0;
            if is_even {
                header_start.0 =
                    page.content_box.x2 - layout::width_of_text(&header, &doc.fonts[0], Pt(12.0));
            }

            // add a line below the header
            let mut content = Content::new();
            content
                .set_stroke_gray(0.75)
                .set_line_cap(LineCapStyle::ButtCap)
                .set_line_width(0.25)
                .move_to(*page.content_box.x1, *header_start.1 - 2.0)
                .line_to(*page.content_box.x2, *header_start.1 - 2.0)
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

            layout::layout_text(&doc, &mut page, start, &mut text, wrap_width, bbox);
            doc.add_page(page);
        }

        Ok(())
    }
}
