use forgen::incremental::IncrementalCache;
use std::path::Path;

#[test]
fn test_incremental_module_freshness_and_invalidation() {
    let mut cache = IncrementalCache::new();
    let mod_a = Path::new("src/module_a.dtr");
    let content_v1 = "fn compute() -> Int => 42\n";

    // Initially, cache is cold
    assert!(!cache.is_module_fresh(mod_a, content_v1));

    // After updating module
    cache.update_module(mod_a, content_v1, vec![]);
    assert!(
        cache.is_module_fresh(mod_a, content_v1),
        "Module must be fresh after updating cache"
    );

    // When content changes, it is no longer fresh
    let content_v2 = "fn compute() -> Int => 84\n";
    assert!(
        !cache.is_module_fresh(mod_a, content_v2),
        "Modified content must invalidate freshness"
    );

    // Updating to v2 makes it fresh again
    cache.update_module(mod_a, content_v2, vec![]);
    assert!(cache.is_module_fresh(mod_a, content_v2));

    // Explicit invalidation
    cache.invalidate_module(mod_a);
    assert!(!cache.is_module_fresh(mod_a, content_v2));
}

#[test]
fn test_incremental_cache_serialization() {
    let temp_dir = std::env::temp_dir().join("forgen_test_cache");
    let mut cache = IncrementalCache::new();
    let mod_b = Path::new("src/module_b.dtr");
    let content = "class User { id Int }\n";

    cache.update_module(mod_b, content, vec!["src/core.dtr".into()]);
    assert!(cache.save_to_dir(&temp_dir).is_ok());

    let loaded = IncrementalCache::load_from_dir(&temp_dir);
    assert!(loaded.is_module_fresh(mod_b, content));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
