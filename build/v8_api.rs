//! Compile against the already-resolved, exactly pinned rusty_v8 dependency (no downloads).
use std::{env, fs, path::PathBuf};

pub fn build() {
    println!("cargo:rerun-if-changed=src/engine/script/engine/v8_api.cc");
    println!("cargo:rerun-if-changed=build/v8_api.rs");
    println!("cargo:rerun-if-env-changed=BREEZE_V8_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    let source = source_directory().expect(
        "V8 152.2.0 headers not found; for vendored/custom Cargo registries set BREEZE_V8_SOURCE_DIR to the resolved v8 crate directory",
    );
    let manifest = fs::read_to_string(source.join("Cargo.toml")).expect("read V8 source manifest");
    let package = manifest
        .split("[package]")
        .nth(1)
        .and_then(|section| section.split("\n[").next())
        .expect("V8 package metadata");
    assert!(
        package
            .lines()
            .any(|line| line.trim() == "version = \"152.2.0\""),
        "V8 header/library version mismatch"
    );
    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .warnings(false)
        .flag_if_supported("/Zc:__cplusplus")
        .include(source.join("v8/include"))
        .define("_ITERATOR_DEBUG_LEVEL", "0")
        .file("src/engine/script/engine/v8_api.cc")
        .compile("breeze_v8_api");
}

fn source_directory() -> Option<PathBuf> {
    if let Some(path) = env::var_os("BREEZE_V8_SOURCE_DIR") {
        return Some(path.into());
    }
    let cargo = env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(|home| PathBuf::from(home).join(".cargo"))
    })?;
    let mut candidates: Vec<_> = fs::read_dir(cargo.join("registry/src"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("v8-152.2.0"))
        .filter(|path| path.join("v8/include/v8.h").is_file())
        .collect();
    // Ambiguous registry replacements need an explicit path, not a guessed ABI.
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}
