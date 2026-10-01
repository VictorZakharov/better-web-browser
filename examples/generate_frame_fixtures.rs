//! Rebuild the tiny owned encoded inputs, with all outputs confined to G:.
#[path = "../src/engine/script/image_frames/test_fixtures.rs"]
mod fixtures;
use base64::Engine;

fn main() {
    let output = std::env::args()
        .nth(1)
        .expect("provide an absolute G: output directory");
    let path = std::path::PathBuf::from(output);
    assert!(
        path.is_absolute()
            && path
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("g:\\"),
        "generated image fixtures must stay on G:"
    );
    std::fs::create_dir_all(&path).unwrap();
    let entries = fixtures::all()
        .into_iter()
        .map(|(name, mime, bytes)| {
            serde_json::json!({"name":name,"type":mime,
            "bytes":base64::engine::general_purpose::STANDARD.encode(bytes)})
        })
        .collect::<Vec<_>>();
    let json = serde_json::to_string_pretty(&entries).unwrap() + "\n";
    std::fs::write(path.join("frames.json"), json).unwrap();
    println!(
        "Generated {} owned image fixtures in {}",
        entries.len(),
        path.display()
    );
}
