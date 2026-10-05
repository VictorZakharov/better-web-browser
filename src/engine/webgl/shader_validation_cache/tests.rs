use super::*;

fn environment() -> Environment {
    Environment {
        api: ApiVersion::One,
        kind: 0x8b31,
        derivatives: false,
        frag_depth: false,
        texture_lod: false,
        draw_buffers: false,
        max_draw_buffers: 4,
    }
}

#[test]
fn compiler_environment_and_exact_source_both_identify_cache_entries() {
    let mut cache = Cache::default();
    let key = environment();
    cache.insert(key, "shader", "validated");
    assert_eq!(cache.lookup(key, "shader"), Some("validated".into()));
    assert_eq!(cache.lookup(key, "shader "), None);
    let variants = [
        Environment {
            api: ApiVersion::Two,
            ..key
        },
        Environment {
            kind: 0x8b30,
            ..key
        },
        Environment {
            derivatives: true,
            ..key
        },
        Environment {
            frag_depth: true,
            ..key
        },
        Environment {
            texture_lod: true,
            ..key
        },
        Environment {
            draw_buffers: true,
            ..key
        },
        Environment {
            max_draw_buffers: 8,
            ..key
        },
    ];
    for changed in variants {
        assert_eq!(cache.lookup(changed, "shader"), None, "{changed:?}");
    }
}

#[test]
fn recent_hits_preserve_entries_while_insertion_evicts_the_oldest() {
    let mut cache = Cache::default();
    for index in 0..MAX_ENTRIES {
        cache.insert(environment(), &index.to_string(), "output");
    }
    assert_eq!(cache.lookup(environment(), "0"), Some("output".into()));
    cache.insert(environment(), "new", "output");
    assert_eq!(cache.lookup(environment(), "1"), None);
    assert_eq!(cache.lookup(environment(), "0"), Some("output".into()));
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
}

#[test]
fn byte_budget_counts_both_source_and_translated_output() {
    let mut cache = Cache::default();
    let source = "a".repeat(MAX_BYTES / 2);
    let translated = "b".repeat(MAX_BYTES / 2);
    cache.insert(environment(), &source, &translated);
    assert_eq!(cache.bytes, MAX_BYTES);
    cache.insert(environment(), "small", "output");
    assert_eq!(cache.bytes, 11);
    assert_eq!(cache.entries.len(), 1);
    cache.insert(environment(), &"a".repeat(MAX_BYTES), "too large");
    assert_eq!(cache.bytes, 11);
    assert_eq!(cache.lookup(environment(), "small"), Some("output".into()));
}

#[test]
fn replacement_and_owned_results_do_not_alias_cached_bytes() {
    let mut cache = Cache::default();
    cache.insert(environment(), "source", "first");
    let mut result = cache.lookup(environment(), "source").unwrap();
    result.clear();
    cache.insert(environment(), "source", "second");
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.bytes, 12);
    assert_eq!(cache.lookup(environment(), "source"), Some("second".into()));
    let mut independent = Cache::default();
    assert_eq!(independent.lookup(environment(), "source"), None);
}
