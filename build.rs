fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Boa and the layout engine both execute standards-defined nested algorithms. Windows' default
    // one-megabyte executable stack is too small for otherwise bounded real-world pages (the
    // Google Search challenge document is one example). Reserve the same eight-megabyte UI stack
    // already used by the hidden WPT path; `/STACK` reserves address space and commits pages only
    // as they are used.
    // https://learn.microsoft.com/cpp/build/reference/stack-stack-allocations
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=better-web-browser=/STACK:8388608");
    }
}
