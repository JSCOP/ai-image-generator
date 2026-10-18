use image_core::error::Category;
use image_core::secrets::{MemoryStore, SecretStore};

#[test]
fn memory_store_roundtrip() {
    let store = MemoryStore::new();
    store.set("p-1", "sk-a").unwrap();
    assert_eq!(store.get("p-1").unwrap().unwrap().expose(), "sk-a");
    store.delete("p-1").unwrap();
    assert!(store.get("p-1").unwrap().is_none());
    store.delete("p-1").unwrap();
}

#[test]
fn memory_store_trims_and_rejects_empty_keys() {
    let store = MemoryStore::new();
    store.set("p-1", "  sk-a \n").unwrap();
    assert_eq!(store.get("p-1").unwrap().unwrap().expose(), "sk-a");
    let err = store.set("p-2", "   ").unwrap_err();
    assert_eq!(err.category, Category::InvalidRequest);
    assert!(store.get("p-2").unwrap().is_none());
}

#[test]
fn memory_store_never_debug_prints_key() {
    let store = MemoryStore::new();
    store.set("p-1", "sk-secret-value").unwrap();
    let shown = format!("{:?}", store.get("p-1").unwrap());
    assert_eq!(shown, "Some(***)");
    assert!(!format!("{store:?}").contains("sk-secret-value"));
}

#[cfg(windows)]
mod keyring {
    use image_core::error::Category;
    use image_core::secrets::{KeyringStore, SecretStore};

    fn isolated_service() -> String {
        format!("ImageStudioTest-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn keyring_store_roundtrip_in_isolated_service() {
        let svc = isolated_service();
        let s = KeyringStore::with_service(&svc);
        s.set("p-test", "sk-x").unwrap();
        let got = s.get("p-test");
        s.delete("p-test").unwrap();
        assert_eq!(got.unwrap().unwrap().expose(), "sk-x");
        assert!(s.get("p-test").unwrap().is_none());
        s.delete("p-test").unwrap();
    }

    #[test]
    fn keyring_store_rejects_empty_and_too_long_keys() {
        let svc = isolated_service();
        let s = KeyringStore::with_service(&svc);
        let empty = s.set("p-test", "").unwrap_err();
        assert_eq!(empty.category, Category::InvalidRequest);
        let long = "k".repeat(1300);
        let err = s.set("p-test", &long).unwrap_err();
        assert_eq!(err.category, Category::InvalidRequest);
        assert!(err.message.contains("키가 너무 깁니다"), "{}", err.message);
        assert!(!err.message.contains(&long));
        assert!(s.get("p-test").unwrap().is_none());
    }
}
