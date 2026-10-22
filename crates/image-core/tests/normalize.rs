use image::{ImageFormat, Rgba, RgbaImage};
use image_core::error::Category;
use image_core::normalize::{alpha_stats, normalize, Fit};
use image_core::size::Size;
use std::io::Cursor;
use test_fixture::png_bytes;

fn encode(img: &RgbaImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let dynamic = image::DynamicImage::ImageRgba8(img.clone());
    if format == ImageFormat::Jpeg {
        dynamic.to_rgb8().write_to(&mut out, format).unwrap();
    } else {
        dynamic.write_to(&mut out, format).unwrap();
    }
    out.into_inner()
}

fn size(width: u32, height: u32) -> Size {
    Size { width, height }
}

/// 1374x1145 transparent canvas with an opaque red bar in the last 6 columns (the "handle").
fn edge_subject() -> Vec<u8> {
    let mut img = RgbaImage::from_pixel(1374, 1145, Rgba([0, 0, 0, 0]));
    for x in 1368..1374 {
        for y in 0..1145 {
            img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }
    encode(&img, ImageFormat::Png)
}

#[test]
fn passthrough_when_png_and_size_match() {
    let png = png_bytes(64, 32, [1, 2, 3, 255]);
    let n = normalize(&png, size(64, 32), Fit::Cover).unwrap();
    assert_eq!(n.png, png);
    assert_eq!(n.fit_applied, "none");
    assert_eq!(n.provider_size, size(64, 32));
}

#[test]
fn cover_crops_center_to_exact_size() {
    let n = normalize(&png_bytes(1536, 1024, [5, 5, 5, 255]), size(1920, 1080), Fit::Cover).unwrap();
    let img = image::load_from_memory(&n.png).unwrap();
    assert_eq!((img.width(), img.height()), (1920, 1080));
    assert_eq!((n.provider_size, n.saved_size, n.fit_applied), (size(1536, 1024), size(1920, 1080), "cover"));
}

#[test]
fn jpeg_input_is_reencoded_png() {
    let jpeg = encode(&RgbaImage::from_pixel(32, 32, Rgba([10, 20, 30, 255])), ImageFormat::Jpeg);
    let n = normalize(&jpeg, size(32, 32), Fit::Cover).unwrap();
    assert_eq!(&n.png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(n.fit_applied, "cover");
}

#[test]
fn contain_keeps_subject_with_transparent_padding() {
    let n = normalize(&edge_subject(), size(1024, 1024), Fit::Contain).unwrap();
    let img = image::load_from_memory(&n.png).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (1024, 1024));
    assert!((1018..1024).any(|x| img.get_pixel(x, 512)[3] > 0));
    assert_eq!(img.get_pixel(512, 0)[3], 0);
    assert_eq!(n.fit_applied, "contain");
}

#[test]
fn cover_loses_edge_subject_which_is_why_contain_exists() {
    let n = normalize(&edge_subject(), size(1024, 1024), Fit::Cover).unwrap();
    let img = image::load_from_memory(&n.png).unwrap().to_rgba8();
    assert!((1018..1024).all(|x| img.get_pixel(x, 512)[3] == 0));
}

#[test]
fn alpha_stats_separate_full_partial_opaque() {
    let mut img = RgbaImage::new(2, 2);
    img.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
    img.put_pixel(1, 0, Rgba([0, 0, 0, 0]));
    img.put_pixel(0, 1, Rgba([0, 0, 0, 253]));
    img.put_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let s = alpha_stats(&encode(&img, ImageFormat::Png)).unwrap();
    assert!(s.has_alpha_channel);
    assert_eq!((s.transparent_ratio, s.partial_ratio, s.opaque_ratio), (0.5, 0.25, 0.25));
    assert_eq!(s.corner_alpha, [0, 0, 253, 255]);
    let jpeg = encode(&RgbaImage::from_pixel(4, 4, Rgba([1, 1, 1, 255])), ImageFormat::Jpeg);
    let s = alpha_stats(&jpeg).unwrap();
    assert!(!s.has_alpha_channel);
    assert_eq!(s.opaque_ratio, 1.0);
}

#[test]
fn undecodable_bytes_are_provider_error() {
    assert_eq!(normalize(b"not an image", size(8, 8), Fit::Cover).unwrap_err().category, Category::Provider);
}

#[test]
fn original_keeps_the_provider_image_and_size() {
    let png = png_bytes(1145, 1374, [9, 9, 9, 255]);
    let n = normalize(&png, size(1920, 1080), Fit::Original).unwrap();
    assert_eq!(n.png, png, "a PNG is saved untouched");
    assert_eq!((n.provider_size, n.saved_size, n.fit_applied), (size(1145, 1374), size(1145, 1374), "none"));

    let jpeg = encode(&RgbaImage::from_pixel(40, 30, Rgba([10, 20, 30, 255])), ImageFormat::Jpeg);
    let n = normalize(&jpeg, size(1920, 1080), Fit::Original).unwrap();
    assert_eq!(&n.png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!((n.saved_size, n.fit_applied), (size(40, 30), "original"));
    assert_eq!(Fit::parse("original"), Some(Fit::Original));
    assert_eq!(Fit::Original.as_str(), "original");
}

/// A provider answer within 1% of the requested shape fills the frame: no 1px transparent seam on an opaque image.
#[test]
fn contain_fills_the_frame_when_the_shape_nearly_matches() {
    let near = png_bytes(1672, 941, [10, 20, 30, 255]);
    let out = normalize(&near, size(1920, 1080), Fit::Contain).unwrap();
    assert_eq!(out.fit_applied, "cover");
    assert_eq!(alpha_stats(&out.png).unwrap().transparent_ratio, 0.0);
    let far = png_bytes(1536, 1024, [10, 20, 30, 255]);
    let out = normalize(&far, size(1920, 1080), Fit::Contain).unwrap();
    assert_eq!(out.fit_applied, "contain");
    assert!(alpha_stats(&out.png).unwrap().transparent_ratio > 0.0);
}
