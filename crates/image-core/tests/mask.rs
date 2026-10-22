use image::{DynamicImage, Rgba, RgbaImage};
use image_core::error::Category;
use image_core::mask::{crop_rect, edit_weights, marked_bounds, prepare, provider_size, Rect};
use image_core::size::Size;

/// Noisy source so a changed pixel cannot match by accident.
fn source(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x + y) % 256) as u8, 255]))
}

/// OpenAI-style mask: alpha 0 inside `area`, 255 elsewhere.
fn mask(w: u32, h: u32, area: Rect) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let inside = x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height;
        Rgba([0, 0, 0, if inside { 0 } else { 255 }])
    })
}

fn save(dir: &std::path::Path, name: &str, image: &RgbaImage) -> std::path::PathBuf {
    let path = dir.join(name);
    image.save(&path).unwrap();
    path
}

#[test]
fn marked_bounds_and_crop_with_context() {
    let area = Rect { x: 100, y: 50, width: 40, height: 20 };
    let weights = edit_weights(&mask(400, 300, area));
    assert_eq!(marked_bounds(&weights), Some(area));
    // 48px minimum context, clamped to the image.
    assert_eq!(crop_rect(area, 400, 300), Rect { x: 52, y: 2, width: 136, height: 116 });
    assert_eq!(crop_rect(Rect { x: 0, y: 0, width: 10, height: 10 }, 30, 30), Rect { x: 0, y: 0, width: 30, height: 30 });
    assert_eq!(marked_bounds(&edit_weights(&RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255])))), None);
    assert_eq!(provider_size(Rect { x: 0, y: 0, width: 300, height: 150 }), Size { width: 1024, height: 512 });
}

#[test]
fn composite_changes_only_the_marked_area() {
    let dir = tempfile::tempdir().unwrap();
    let (w, h) = (200, 160);
    let area = Rect { x: 60, y: 40, width: 50, height: 50 };
    let src = source(w, h);
    let edit = prepare(&save(dir.path(), "src.png", &src), &save(dir.path(), "mask.png", &mask(w, h, area)), Size { width: w, height: h })
        .unwrap();
    // The provider answers at another scale; it is fitted back onto the crop.
    let provider = DynamicImage::ImageRgba8(RgbaImage::from_pixel(edit.crop.width * 3, edit.crop.height * 3, Rgba([255, 0, 0, 255])));
    let out = edit.composite(&provider);
    assert_eq!(out.dimensions(), (w, h));
    let mut changed_inside = 0;
    for (x, y, px) in out.enumerate_pixels() {
        let inside = x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height;
        if !inside {
            assert_eq!(px, src.get_pixel(x, y), "pixel outside the mask changed at {x},{y}");
        } else if px != src.get_pixel(x, y) {
            changed_inside += 1;
        }
    }
    assert_eq!(*out.get_pixel(85, 65), Rgba([255, 0, 0, 255]), "the middle of the area is fully edited");
    assert!(changed_inside > 2400, "{changed_inside}");
    // The edge is feathered inward: the first marked pixel is only partly edited.
    assert_ne!(*out.get_pixel(60, 65), Rgba([255, 0, 0, 255]));
}

#[test]
fn prepare_rejects_bad_masks() {
    let dir = tempfile::tempdir().unwrap();
    let src = save(dir.path(), "src.png", &source(64, 48));
    let area = Rect { x: 1, y: 1, width: 5, height: 5 };
    let size = Size { width: 64, height: 48 };
    let cases = [
        (save(dir.path(), "small.png", &mask(32, 48, area)), size, "must match"),
        (save(dir.path(), "empty.png", &RgbaImage::from_pixel(64, 48, Rgba([0, 0, 0, 255]))), size, "no area"),
        (save(dir.path(), "ok.png", &mask(64, 48, area)), Size { width: 1024, height: 1024 }, "set size to 64x48"),
    ];
    for (mask_path, size, text) in cases {
        let err = prepare(&src, &mask_path, size).err().expect(text);
        assert_eq!(err.category, Category::InvalidRequest);
        assert!(err.message.contains(text), "{}", err.message);
    }
}
