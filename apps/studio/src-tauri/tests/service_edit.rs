//! Mask edit layers → engine mask and drawn source.

use base64::Engine;
use image::{Rgba, RgbaImage};
use image_core::error::Category;
use studio_lib::service::edit::{write_edit_files, EditInput};

fn b64(image: &RgbaImage) -> String {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out.into_inner()))
}

/// Transparent layer with `rgba` painted over x 2..5, y 1..3.
fn layer(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| if (2..5).contains(&x) && (1..3).contains(&y) { Rgba(rgba) } else { Rgba([0, 0, 0, 0]) })
}

fn source(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("src.png");
    RgbaImage::from_pixel(8, 6, Rgba([10, 20, 30, 255])).save(&path).unwrap();
    path
}

#[test]
fn painted_area_becomes_an_openai_mask_next_to_the_records() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let records = dir.path().join("metadata/t");
    let edit = EditInput { mask: b64(&layer(8, 6, [255, 0, 0, 128])), strokes: None };
    let files = write_edit_files(&src, &edit, &records, "t").unwrap();
    assert_eq!(files.reference, src, "without a drawing the source itself is sent");
    assert_eq!(files.mask, records.join("t-mask.png"));
    let mask = image::open(&files.mask).unwrap().to_rgba8();
    assert_eq!(mask.get_pixel(3, 2)[3], 127, "painted alpha 128 → edit weight 128");
    assert_eq!(mask.get_pixel(0, 0)[3], 255);
    // A retry with the same layers reuses the files.
    let again = write_edit_files(&src, &edit, &records, "t").unwrap();
    assert!(again.written.is_empty());
    // Different content under the same name is never replaced.
    let other = EditInput { mask: b64(&layer(8, 6, [255, 0, 0, 255])), strokes: None };
    assert_eq!(write_edit_files(&src, &other, &records, "t").unwrap_err().category, Category::Conflict);
}

#[test]
fn a_drawing_is_sent_on_the_image_and_counts_as_marked() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let records = dir.path().join("metadata/t");
    let empty = RgbaImage::new(8, 6);
    let edit = EditInput { mask: b64(&empty), strokes: Some(b64(&layer(8, 6, [0, 255, 0, 255]))) };
    let files = write_edit_files(&src, &edit, &records, "t").unwrap();
    assert_eq!(files.reference, records.join("t-drawn.png"));
    let drawn = image::open(&files.reference).unwrap().to_rgba8();
    assert_eq!(*drawn.get_pixel(3, 2), Rgba([0, 255, 0, 255]));
    assert_eq!(*drawn.get_pixel(0, 0), Rgba([10, 20, 30, 255]));
    assert_eq!(image::open(&files.mask).unwrap().to_rgba8().get_pixel(3, 2)[3], 0);
}

#[test]
fn bad_layers_are_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let src = source(dir.path());
    let records = dir.path().join("metadata/t");
    let cases = [
        EditInput { mask: b64(&RgbaImage::new(8, 6)), strokes: None },
        EditInput { mask: b64(&layer(4, 6, [0, 0, 0, 255])), strokes: None },
        EditInput { mask: "not base64!".into(), strokes: None },
    ];
    for edit in cases {
        assert_eq!(write_edit_files(&src, &edit, &records, "t").unwrap_err().category, Category::InvalidRequest);
    }
    assert!(!records.exists());
}
