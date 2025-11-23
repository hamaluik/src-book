//! Image file rendering.
//!
//! Displays images (PNG, JPG, SVG, etc.) centred on the page with file metadata.

use crate::sinks::pdf::config::PDF;
use crate::sinks::pdf::fonts::FontIds;
use crate::sinks::pdf::rendering::ImagePathMap;
use anyhow::Result;
use chrono::TimeZone;
use pdf_gen::layout::Margins;
use pdf_gen::*;
use std::path::Path;

/// Render an image file as a full page with header and metadata.
///
/// Records the image path in `image_paths` so booklet rendering can reload the image.
pub fn render(
    config: &PDF,
    doc: &mut Document,
    font_ids: &FontIds,
    path: &Path,
    image_paths: &mut ImagePathMap,
) -> Result<usize> {
    let subheading_size = Pt(config.font_size_subheading_pt);
    let small_size = Pt(config.font_size_small_pt);

    let image = Image::new_from_disk(path)?;
    let aspect_ratio = image.aspect_ratio();
    let image_id = doc.add_image(image);
    let image_index = image_id.index();

    // record path for booklet rendering
    image_paths.insert(image_index, path.to_path_buf());

    let margins = Margins::trbl(
        In(0.25).into(),
        In(0.25).into(),
        In(0.5).into(),
        In(0.25).into(),
    )
    .with_gutter(In(0.25).into(), doc.page_order.len());
    let mut page = Page::new(config.page_size(), Some(margins));

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

    let x = (page.content_box.x2 - page.content_box.x1 - image_size.0) / 2.0 + page.content_box.x1;
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
    let (file_description, image_description) = describe_image(&doc.images[image_id], path);
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

fn describe_image(image: &Image, path: &Path) -> (String, String) {
    let mut file_description: String = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if let Ok(metadata) = std::fs::metadata(path) {
        let file_size = metadata.len();
        let file_size = byte_unit::Byte::from_u128(file_size as u128)
            .expect("can create byte unit from file size");
        let file_size = file_size
            .get_appropriate_unit(byte_unit::UnitType::Binary)
            .to_string();
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
            let size = tree.size();
            let w = size.width();
            let h = size.height();
            image_description.push_str(&format!("SVG size: {w}x{h}"));
        }
    }

    (file_description, image_description)
}
