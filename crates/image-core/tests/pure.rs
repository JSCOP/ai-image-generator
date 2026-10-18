use image_core::catalog::{backend, catalog, supports_native_transparency, supports_references, Backend};
use image_core::paths::{gallery_root, metadata_path, safe_topic_slug, topic_dirs};
use image_core::size::{gemini_aspect_ratio, gemini_image_size, parse_size, Size};

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    image_core::paths::absolute(path)
}

#[test]
fn slug_matches_python() {
    assert_eq!(safe_topic_slug("  my topic/v2! "), "my_topic_v2");
    assert_eq!(safe_topic_slug("공항 보안 검색대"), "공항_보안_검색대");
    assert_eq!(safe_topic_slug("../.."), "image-request");
    assert_eq!(safe_topic_slug(r"C:\x"), "C_x");
    assert_eq!(safe_topic_slug(&"가".repeat(100)).chars().count(), 80);
}

#[test]
fn sizes() {
    assert_eq!(parse_size("1920x1080").unwrap(), Size { width: 1920, height: 1080 });
    assert_eq!(parse_size("1920X1080").unwrap().width, 1920);
    assert_eq!(parse_size("0x5").unwrap_err(), "size 0x5: width and height must be positive");
    assert_eq!(parse_size("big").unwrap_err(), "invalid size 'big'; expected WIDTHxHEIGHT");
    assert_eq!(parse_size("1000x1000").unwrap().padded16().to_string(), "1008x1008");
    assert_eq!(gemini_aspect_ratio(parse_size("1920x1080").unwrap()), "16:9");
    assert_eq!(gemini_image_size(parse_size("768x768").unwrap()), "512");
    assert_eq!(gemini_image_size(parse_size("1536x1024").unwrap()), "1K");
    assert_eq!(gemini_image_size(parse_size("3072x10").unwrap()), "2K");
    assert_eq!(gemini_image_size(parse_size("3840x2160").unwrap()), "4K");
}

#[test]
fn backends_and_capabilities() {
    assert!(matches!(backend("gemini-3.1-flash-image"), Backend::Gemini));
    assert!(matches!(backend("grok-imagine-image-2.0"), Backend::OpenAiImages));
    assert!(matches!(backend("gpt-5.5"), Backend::Unsupported));
    assert!(!supports_references("grok-imagine-image"));
    assert!(supports_references("gpt-image-2.5"));
    assert!(supports_native_transparency("gpt-image-2.5-sunburst"));
    assert!(supports_native_transparency("gpt-image-2.5-flare"));
    assert!(!supports_native_transparency("gpt-image-2.5"));
    assert_eq!(catalog().len(), 9);
    assert!(image_core::catalog::is_listable_image_model("grok-imagine-image-quality"));
    assert!(!image_core::catalog::is_listable_image_model("gpt-5.5"));
}

#[test]
fn gallery_paths() {
    let ws = tempfile::tempdir().unwrap();
    let root = canonical(ws.path());
    let (out, meta) = topic_dirs(ws.path(), "t");
    assert_eq!(out, root.join("ImageGallery").join("output").join("t"));
    assert_eq!(meta, root.join("ImageGallery").join("metadata").join("t"));
    let nested = ws.path().join("ImageGallery").join("output");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(gallery_root(&nested), root.join("ImageGallery"));
    assert_eq!(metadata_path(&out.join("a.png"), ws.path(), "t"), meta.join("a.png.json"));
    assert!(!root.to_string_lossy().starts_with(r"\\?\"));
}
