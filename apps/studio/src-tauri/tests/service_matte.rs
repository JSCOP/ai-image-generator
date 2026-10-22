//! Background removal: model download and verification, pre/post-processing, and saving the cut-out.

use std::sync::{Arc, Mutex};

use image::{GrayImage, Luma, Rgba, RgbaImage};
use image_core::error::Category;
use sha2::{Digest, Sha256};
use studio_lib::service::matte::{alpha_from_prediction, apply_alpha, ensure_model, model_input, save_cut_out, MatteEvent, MatteModel};
use test_fixture::{FakeProvider, Reply};

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn model_for(url: &'static str, body: &[u8]) -> MatteModel {
    let sha: String = Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect();
    MatteModel { name: "fake", file: "fake.onnx", url, sha256: leak(sha), bytes: body.len() as u64, license: "MIT" }
}

fn binary(body: &[u8]) -> Reply {
    Reply { status: 200, headers: vec![], body: body.to_vec(), delay: std::time::Duration::ZERO }
}

#[tokio::test]
async fn model_is_downloaded_checked_and_reused() {
    const BODY: &[u8] = b"pretend onnx model bytes";
    let p = FakeProvider::start(|_| binary(BODY));
    let model = model_for(leak(format!("{}/model.onnx", p.base_url())), BODY);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("models/fake.onnx");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    ensure_model(&dest, &model, &move |e| sink.lock().unwrap().push(e)).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), BODY);
    let total = BODY.len() as u64;
    assert_eq!(events.lock().unwrap().last(), Some(&MatteEvent::Downloading { done: total, total }));
    ensure_model(&dest, &model, &|_| {}).await.unwrap();
    assert_eq!(p.requests().len(), 1, "a verified model is not downloaded again");
}

#[tokio::test]
async fn a_wrong_download_is_deleted_and_fails() {
    let p = FakeProvider::start(|_| binary(b"tampered"));
    let model = model_for(leak(format!("{}/model.onnx", p.base_url())), b"expected");
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("models/fake.onnx");
    let err = ensure_model(&dest, &model, &|_| {}).await.unwrap_err();
    assert_eq!(err.category, Category::Provider);
    assert!(!dest.exists());
    assert_eq!(std::fs::read_dir(dir.path().join("models")).unwrap().count(), 0, "no .part left");
}

#[test]
fn pre_and_post_processing() {
    let image = RgbaImage::from_pixel(4, 2, Rgba([200, 100, 0, 255]));
    let input = model_input(&image);
    assert_eq!(input.len(), 3 * 1024 * 1024);
    assert!((input[0] - 0.5).abs() < 1e-6, "brightest value → 1.0 - 0.5");
    assert!((input[2 * 1024 * 1024] + 0.5).abs() < 1e-6, "0 → -0.5");
    let mut prediction = vec![-3.0f32; 1024 * 1024];
    prediction[..512 * 1024].fill(5.0);
    let alpha = alpha_from_prediction(&prediction, 8, 8);
    assert_eq!((alpha.get_pixel(0, 0)[0], alpha.get_pixel(0, 7)[0]), (255, 0));
    let mut rgba = RgbaImage::from_pixel(2, 1, Rgba([1, 2, 3, 100]));
    rgba.put_pixel(1, 0, Rgba([1, 2, 3, 255]));
    let out = apply_alpha(&rgba, &GrayImage::from_pixel(2, 1, Luma([180])));
    assert_eq!((out.get_pixel(0, 0)[3], out.get_pixel(1, 0)[3]), (100, 180), "never more opaque than before");
}

#[test]
fn cut_out_is_a_new_gallery_image_with_a_record() {
    let work = tempfile::tempdir().unwrap();
    let source = work.path().join("photo.png");
    RgbaImage::from_pixel(4, 4, Rgba([9, 9, 9, 255])).save(&source).unwrap();
    let mut cut = RgbaImage::from_pixel(4, 4, Rgba([9, 9, 9, 255]));
    cut.put_pixel(0, 0, Rgba([9, 9, 9, 0]));
    let first = save_cut_out(work.path(), &source, &cut).unwrap();
    let second = save_cut_out(work.path(), &source, &cut).unwrap();
    assert_ne!(first.output, second.output, "a second cut-out never replaces the first");
    assert!(first.topic.starts_with("photo-nobg-"));
    assert_eq!(first.transparent_ratio, 1.0 / 16.0);
    let record_path = work.path().join(format!("ImageGallery/metadata/{0}/{0}.png.json", first.topic));
    let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(record_path).unwrap()).unwrap();
    assert_eq!(record["action"], "remove_background");
    assert_eq!(record["image_model"], "isnet-general-use");
    assert_eq!(record["status"], "complete");
    assert_eq!(record["background"], "transparent");
    assert!(record["reference_images"][0].as_str().unwrap().ends_with("photo.png"));
    assert_eq!(image::open(&source).unwrap().to_rgba8().get_pixel(0, 0)[3], 255, "the source is untouched");
}

/// Real model on a real image: set MATTE_MODEL (the .onnx) and MATTE_IMAGE.
#[test]
#[ignore]
fn real_cut_out() {
    let model = std::path::PathBuf::from(std::env::var("MATTE_MODEL").unwrap());
    let source = std::path::PathBuf::from(std::env::var("MATTE_IMAGE").unwrap());
    let work = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    let result = studio_lib::service::matte::cut_out(work.path(), &model, &source).unwrap();
    println!("{} transparent {:.3} in {:?}", result.output, result.transparent_ratio, started.elapsed());
    assert!(result.transparent_ratio > 0.05);
}
