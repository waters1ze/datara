use forgen::incremental::IncrementalCache;
use std::fs;
use std::time::Instant;

#[test]
fn test_incremental_multimodule_dependency_invalidation() {
    let temp_dir = std::env::temp_dir().join("forgen_inc_tree_test");
    let _ = fs::create_dir_all(&temp_dir);

    let mod_a = temp_dir.join("a.dtr");
    let mod_b = temp_dir.join("b.dtr");
    let mod_c = temp_dir.join("c.dtr");
    let mod_d = temp_dir.join("d.dtr");

    let src_a1 = "fn a_calc() -> Int => b_calc() * 2";
    let src_b1 = "fn b_calc() -> Int => c_calc() + 1";
    let src_c1 = "fn c_calc() -> Int => d_calc() + 5";
    let src_d1 = "fn d_calc() -> Int => 100";

    fs::write(&mod_a, src_a1).unwrap();
    fs::write(&mod_b, src_b1).unwrap();
    fs::write(&mod_c, src_c1).unwrap();
    fs::write(&mod_d, src_d1).unwrap();

    let mut cache = IncrementalCache::new();

    // 1. Initial build: cache all 4 modules
    cache.update_module(&mod_a, src_a1, vec!["b".into()]);
    cache.update_module(&mod_b, src_b1, vec!["c".into()]);
    cache.update_module(&mod_c, src_c1, vec!["d".into()]);
    cache.update_module(&mod_d, src_d1, vec![]);

    assert!(cache.is_module_fresh(&mod_a, src_a1));
    assert!(cache.is_module_fresh(&mod_b, src_b1));
    assert!(cache.is_module_fresh(&mod_c, src_c1));
    assert!(cache.is_module_fresh(&mod_d, src_d1));

    // 2. Modify module C (c.dtr)
    let start_change = Instant::now();
    let src_c2 = "fn c_calc() -> Int => d_calc() + 99";
    fs::write(&mod_c, src_c2).unwrap();

    // D remains untouched -> Cache HIT on D
    assert!(
        cache.is_module_fresh(&mod_d, src_d1),
        "Module D must be a fresh cache hit"
    );

    // C is modified -> Cache MISS on C
    assert!(
        !cache.is_module_fresh(&mod_c, src_c2),
        "Module C must be invalidated and require recompilation"
    );

    // Update C in cache
    cache.update_module(&mod_c, src_c2, vec!["d".into()]);
    assert!(cache.is_module_fresh(&mod_c, src_c2));

    let invalidation_us = start_change.elapsed().as_micros();
    println!(
        "Incremental dependency invalidation completed in {}µs",
        invalidation_us
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
