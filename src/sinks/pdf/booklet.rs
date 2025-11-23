//! Booklet PDF generation for saddle-stitch binding.
//!
//! Converts the digital PDF into a print-ready booklet by:
//! 1. Converting each page into a Form XObject for reuse
//! 2. Calculating imposition layout (which pages go where on physical sheets)
//! 3. Placing two logical pages side-by-side on each sheet side
//!
//! The output is designed for duplex printing: print the PDF, fold the stack
//! in half, and staple along the spine.
//!
//! ## Document Metadata
//!
//! The booklet PDF receives the same document metadata as the main PDF (title,
//! author, subject, keywords, creator) with " (Booklet)" appended to the title
//! to distinguish it in file browsers and PDF viewers.
//!
//! ## Image Handling
//!
//! Images cannot be directly cloned between PDF documents because `ImageLayout`
//! stores only an arena index, not the image data. To support images in booklets,
//! the main render pass tracks each image's source path in an `ImagePathMap`.
//! During booklet generation, images are reloaded from disk and assigned new
//! indices. A remapping table ensures each unique image is loaded only once,
//! even if it appears on multiple pages.
//!
//! Displays a progress bar during XObject creation since this can take time
//! for large documents (one XObject per page).

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::{FontIds, LoadedFonts};
use crate::sinks::pdf::imposition::{calculate_imposition, create_imposed_page, BookletConfig};
use crate::sinks::pdf::rendering::ImagePathMap;
use crate::source::Source;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use pdf_gen::id_arena_crate::Id;
use pdf_gen::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// Generate a print-ready booklet PDF from the digital document.
///
/// This creates Form XObjects from each page's content and arranges them
/// 2-up on larger sheets according to saddle-stitch signature imposition.
///
/// The `source` parameter provides book metadata (title, authors) for setting
/// PDF document properties. Images are reloaded from disk using paths recorded
/// in `image_paths` during initial rendering, then remapped to new indices.
///
/// Returns the number of physical sheets needed to print the booklet.
pub fn render_booklet(
    config: &PDF,
    source: &Source,
    source_doc: &Document,
    source_font_ids: &FontIds,
    image_paths: &ImagePathMap,
    output_path: &PathBuf,
) -> Result<usize> {
    let page_width = Pt(config.page_width_in * 72.0);
    let page_height = Pt(config.page_height_in * 72.0);
    let sheet_width = Pt(config.booklet_sheet_width_in * 72.0);
    let sheet_height = Pt(config.booklet_sheet_height_in * 72.0);

    let booklet_config = BookletConfig {
        signature_size: config.booklet_signature_size,
        sheet_width,
        sheet_height,
        page_width,
        page_height,
    };

    // create a new document for the booklet
    let mut booklet_doc = Document::default();

    // set PDF metadata for the booklet
    let mut info = Info::default();
    if let Some(title) = &source.title {
        info.title(format!("{} (Booklet)", title));
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
    if let Some(subject) = &config.subject {
        info.subject(subject);
    }
    if let Some(keywords) = &config.keywords {
        info.keywords(keywords);
    }
    info.creator(concat!("src-book v", env!("CARGO_PKG_VERSION")));
    booklet_doc.set_info(info);

    // reload fonts for the booklet document (fonts can't be cloned)
    let fonts = LoadedFonts::load(&config.font)
        .with_context(|| format!("Failed to reload font '{}' for booklet", config.font))?;
    let booklet_font_ids = FontIds {
        regular: booklet_doc.add_font(fonts.regular),
        bold: booklet_doc.add_font(fonts.bold),
        italic: booklet_doc.add_font(fonts.italic),
        bold_italic: booklet_doc.add_font(fonts.bold_italic),
    };

    // maps source image indices to booklet document image indices
    let mut image_remap: HashMap<usize, usize> = HashMap::new();

    // create Form XObjects from each source page
    let progress = ProgressBar::new(source_doc.page_order.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("can parse progress style")
            .progress_chars("#>-"),
    );
    progress.set_message("Creating booklet...");

    let mut page_xobjs: Vec<Id<FormXObject>> = Vec::new();
    for page_id in source_doc.page_order.iter() {
        let page = &source_doc.pages[*page_id];
        let mut xobj = FormXObject::new(page.media_box.x2, page.media_box.y2);

        // copy page contents to the form xobject
        for content in page.contents.iter() {
            match content {
                PageContents::Text(spans) => {
                    for span in spans {
                        // remap font ids to the booklet document
                        let new_font_id = if span.font.id == source_font_ids.regular {
                            booklet_font_ids.regular
                        } else if span.font.id == source_font_ids.bold {
                            booklet_font_ids.bold
                        } else if span.font.id == source_font_ids.italic {
                            booklet_font_ids.italic
                        } else {
                            booklet_font_ids.bold_italic
                        };
                        xobj.add_span(SpanLayout {
                            text: span.text.clone(),
                            font: SpanFont {
                                id: new_font_id,
                                size: span.font.size,
                            },
                            colour: span.colour,
                            coords: span.coords,
                        });
                    }
                }
                PageContents::Image(img) => {
                    // remap image index or load from disk if not yet in booklet doc
                    let new_index = if let Some(&idx) = image_remap.get(&img.image_index) {
                        idx
                    } else if let Some(path) = image_paths.get(&img.image_index) {
                        let image = Image::new_from_disk(path).with_context(|| {
                            format!("Failed to reload image '{}' for booklet", path.display())
                        })?;
                        let new_id = booklet_doc.add_image(image);
                        let new_idx = new_id.index();
                        image_remap.insert(img.image_index, new_idx);
                        new_idx
                    } else {
                        // image path not recorded; skip this image
                        continue;
                    };

                    xobj.add_image(ImageLayout {
                        image_index: new_index,
                        position: img.position,
                    });
                }
                PageContents::RawContent(raw) => {
                    xobj.add_raw_content(raw.clone());
                }
                PageContents::FormXObject(_) => {
                    // nested form xobjects not supported in this context
                }
            }
        }

        let xobj_id = booklet_doc.add_form_xobject(xobj);
        page_xobjs.push(xobj_id);
        progress.inc(1);
    }
    progress.finish_with_message("Booklet created");

    // calculate imposition layout
    let total_pages = page_xobjs.len();
    let sheets = calculate_imposition(total_pages, config.booklet_signature_size);

    let sheet_count = sheets.len();

    // create imposed pages (each sheet side becomes a page)
    for sheet in sheets.iter() {
        // front side
        let front_left = sheet.front.left_page.map(|idx| page_xobjs[idx]);
        let front_right = sheet.front.right_page.map(|idx| page_xobjs[idx]);
        let front_page = create_imposed_page(&booklet_config, front_left, front_right);
        booklet_doc.add_page(front_page);

        // back side
        let back_left = sheet.back.left_page.map(|idx| page_xobjs[idx]);
        let back_right = sheet.back.right_page.map(|idx| page_xobjs[idx]);
        let back_page = create_imposed_page(&booklet_config, back_left, back_right);
        booklet_doc.add_page(back_page);
    }

    // write the booklet PDF
    let file = std::fs::File::create(output_path).with_context(|| {
        format!(
            "Failed to create booklet output file: {}",
            output_path.display()
        )
    })?;
    let mut file = std::io::BufWriter::new(file);
    booklet_doc
        .write(&mut file)
        .with_context(|| "Failed to write booklet PDF")?;

    Ok(sheet_count)
}
