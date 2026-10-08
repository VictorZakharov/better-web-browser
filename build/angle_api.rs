//! The tiny delegate adapter uses the existing pinned ANGLE public header, not a guessed ABI.
use std::{env, fs, path::PathBuf};

pub fn build() {
    println!("cargo:rerun-if-changed=build/angle_api.rs");
    println!("cargo:rerun-if-changed=src/engine/webgl/compiler_delegate.cc");
    println!("cargo:rerun-if-env-changed=BREEZE_ANGLE_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    let source = source_directory().expect(
        "mozangle 0.7.1 headers not found; for vendored/custom registries set BREEZE_ANGLE_SOURCE_DIR to the resolved mozangle crate directory",
    );
    let manifest = fs::read_to_string(source.join("Cargo.toml")).expect("read ANGLE manifest");
    let package = manifest
        .split("[package]")
        .nth(1)
        .and_then(|section| section.split("\n[").next())
        .expect("ANGLE package metadata");
    assert!(
        package
            .lines()
            .any(|line| line.trim() == "version = \"0.7.1\""),
        "ANGLE header/library version mismatch"
    );
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .warnings(true)
        // The static provider's public struct is not imported from a DLL.
        .define("ANGLE_PLATFORM_EXPORT", "")
        .define("_ITERATOR_DEBUG_LEVEL", "0")
        .include(source.join("gfx/angle/checkout/include"))
        .file("src/engine/webgl/compiler_delegate.cc")
        .compile("breeze_angle_api");
}

fn source_directory() -> Option<PathBuf> {
    if let Some(path) = env::var_os("BREEZE_ANGLE_SOURCE_DIR") {
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
        .map(|entry| entry.path().join("mozangle-0.7.1"))
        .filter(|path| {
            path.join("gfx/angle/checkout/include/platform/PlatformMethods.h")
                .is_file()
        })
        .collect();
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}
