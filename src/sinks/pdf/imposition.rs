//! Saddle-stitch booklet imposition calculations.
//!
//! Imposition is the process of arranging pages on physical sheets so they appear
//! in the correct order after printing, folding, and binding. For saddle-stitch
//! binding, sheets are nested inside each other and stapled along the spine.
//!
//! A **signature** is a group of nested sheets that form one section of the book.
//! Each signature contains N logical pages (where N must be divisible by 4) printed
//! on N/4 physical sheets.
//!
//! The imposition formula ensures that when sheets are stacked outer-to-inner and
//! folded, pages 1, 2, 3, ... N appear in sequence.

use pdf_gen::id_arena_crate::Id;
use pdf_gen::{FormXObject, FormXObjectLayout, Page, Pt, Transform};

/// Configuration for booklet imposition
#[allow(dead_code)]
pub struct BookletConfig {
    /// Number of pages per signature (must be divisible by 4)
    pub signature_size: u32,
    /// Width of the physical sheet in points
    pub sheet_width: Pt,
    /// Height of the physical sheet in points
    pub sheet_height: Pt,
    /// Width of each logical page in points
    pub page_width: Pt,
    /// Height of each logical page in points
    pub page_height: Pt,
}

/// Represents a single side of a printed sheet (front or back)
pub struct SheetSide {
    /// Left page index (None for blank)
    pub left_page: Option<usize>,
    /// Right page index (None for blank)
    pub right_page: Option<usize>,
}

/// Represents a complete printed sheet (both sides)
pub struct PrintSheet {
    pub front: SheetSide,
    pub back: SheetSide,
}

/// Calculate the signature layout for booklet imposition.
///
/// For saddle-stitch binding, pages must be arranged so that when the sheets
/// are stacked and folded, the pages appear in the correct order.
///
/// For a signature of N pages (N must be divisible by 4):
/// - There are N/2 sheets per signature
/// - Each sheet has 2 pages on front, 2 on back
/// - The folded booklet has pages in order 1, 2, 3, ..., N
///
/// Sheet layout for a 16-page signature:
/// - Sheet 1 Front: pages 16, 1 (left, right)
/// - Sheet 1 Back:  pages 2, 15 (left, right)
/// - Sheet 2 Front: pages 14, 3 (left, right)
/// - Sheet 2 Back:  pages 4, 13 (left, right)
/// - etc.
pub fn calculate_signature_sheets(signature_size: u32) -> Vec<PrintSheet> {
    assert!(signature_size % 4 == 0, "signature size must be divisible by 4");
    assert!(signature_size > 0, "signature size must be positive");

    let num_sheets = signature_size / 2;
    let mut sheets = Vec::with_capacity(num_sheets as usize);

    for sheet_idx in 0..num_sheets {
        let s = sheet_idx as usize;
        let n = signature_size as usize;

        // front: left = n - 2*s, right = 2*s + 1
        // back:  left = 2*s + 2, right = n - 2*s - 1
        // convert to 0-indexed
        let front_left = n - 2 * s - 1;
        let front_right = 2 * s;
        let back_left = 2 * s + 1;
        let back_right = n - 2 * s - 2;

        sheets.push(PrintSheet {
            front: SheetSide {
                left_page: Some(front_left),
                right_page: Some(front_right),
            },
            back: SheetSide {
                left_page: Some(back_left),
                right_page: Some(back_right),
            },
        });
    }

    sheets
}

/// Calculate the complete imposition layout for all pages.
///
/// Takes the total number of logical pages and breaks them into signatures,
/// padding with blank pages if necessary to fill the last signature.
pub fn calculate_imposition(total_pages: usize, signature_size: u32) -> Vec<PrintSheet> {
    let sig_size = signature_size as usize;

    // round up to the nearest signature
    let num_signatures = (total_pages + sig_size - 1) / sig_size;
    let _padded_total = num_signatures * sig_size;

    let mut all_sheets = Vec::new();

    for sig_idx in 0..num_signatures {
        let sig_start = sig_idx * sig_size;
        let base_sheets = calculate_signature_sheets(signature_size);

        for sheet in base_sheets {
            // remap page indices from signature-local to global
            // and replace with None if beyond total_pages
            let remap = |local_idx: Option<usize>| -> Option<usize> {
                local_idx.and_then(|idx| {
                    let global = sig_start + idx;
                    if global < total_pages {
                        Some(global)
                    } else {
                        None
                    }
                })
            };

            all_sheets.push(PrintSheet {
                front: SheetSide {
                    left_page: remap(sheet.front.left_page),
                    right_page: remap(sheet.front.right_page),
                },
                back: SheetSide {
                    left_page: remap(sheet.back.left_page),
                    right_page: remap(sheet.back.right_page),
                },
            });
        }
    }

    all_sheets
}

/// Create a booklet page with two logical pages placed side by side.
///
/// The left page is placed at x=0, the right page at x=page_width.
/// Both are scaled to fit within the sheet dimensions.
pub fn create_imposed_page(
    config: &BookletConfig,
    left_xobj: Option<Id<FormXObject>>,
    right_xobj: Option<Id<FormXObject>>,
) -> Page {
    let mut page = Page::new(
        (config.sheet_width, config.sheet_height),
        None,
    );

    // calculate scaling to fit pages side by side
    // each page gets half the sheet width
    let available_width = config.sheet_width / 2.0;
    let available_height = config.sheet_height;

    let scale_x = *available_width / *config.page_width;
    let scale_y = *available_height / *config.page_height;
    let scale = scale_x.min(scale_y);

    // centre the pages vertically if there's extra space
    let scaled_height = *config.page_height * scale;
    let y_offset = (*available_height - scaled_height) / 2.0;

    // place left page
    if let Some(xobj_id) = left_xobj {
        let transform = Transform::translate(Pt(0.0), Pt(y_offset))
            .with_scale(scale, scale);
        page.add_form_xobject(FormXObjectLayout {
            xobj_id,
            transform,
        });
    }

    // place right page
    if let Some(xobj_id) = right_xobj {
        let transform = Transform::translate(Pt(*available_width), Pt(y_offset))
            .with_scale(scale, scale);
        page.add_form_xobject(FormXObjectLayout {
            xobj_id,
            transform,
        });
    }

    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_sheets_16_pages() {
        let sheets = calculate_signature_sheets(16);
        assert_eq!(sheets.len(), 8);

        // sheet 0 front: 15, 0 (pages 16, 1 in 1-indexed)
        assert_eq!(sheets[0].front.left_page, Some(15));
        assert_eq!(sheets[0].front.right_page, Some(0));
        // sheet 0 back: 1, 14 (pages 2, 15 in 1-indexed)
        assert_eq!(sheets[0].back.left_page, Some(1));
        assert_eq!(sheets[0].back.right_page, Some(14));

        // sheet 1 front: 13, 2 (pages 14, 3 in 1-indexed)
        assert_eq!(sheets[1].front.left_page, Some(13));
        assert_eq!(sheets[1].front.right_page, Some(2));

        // sheet 3 (middle) back: 7, 8 (pages 8, 9 in 1-indexed)
        assert_eq!(sheets[3].back.left_page, Some(7));
        assert_eq!(sheets[3].back.right_page, Some(8));
    }

    #[test]
    fn test_imposition_with_padding() {
        // 20 pages with signature size 16 = 2 signatures (32 pages padded)
        let sheets = calculate_imposition(20, 16);
        assert_eq!(sheets.len(), 16); // 8 sheets per signature * 2 signatures

        // first signature should have all real pages
        assert!(sheets[0].front.left_page.is_some());
        assert!(sheets[0].front.right_page.is_some());

        // second signature will have some blanks (pages 20-31 are blank)
        // sheet 8 front: left=31 (blank), right=16
        assert_eq!(sheets[8].front.left_page, None); // page 31 doesn't exist
        assert_eq!(sheets[8].front.right_page, Some(16));
    }

    #[test]
    fn test_signature_sheets_4_pages() {
        let sheets = calculate_signature_sheets(4);
        assert_eq!(sheets.len(), 2);

        // sheet 0 front: 3, 0 (pages 4, 1)
        assert_eq!(sheets[0].front.left_page, Some(3));
        assert_eq!(sheets[0].front.right_page, Some(0));
        // sheet 0 back: 1, 2 (pages 2, 3)
        assert_eq!(sheets[0].back.left_page, Some(1));
        assert_eq!(sheets[0].back.right_page, Some(2));
    }
}
