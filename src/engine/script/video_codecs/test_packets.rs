//! Test-only IVF storage extraction for our original eight-frame video.
use base64::Engine;
pub(in crate::engine::script) fn fixture(name: &str) -> Vec<u8> {
    let text = match name {
        "ivf" => include_str!("../../../../tests/video-codec-fixtures/eight-frames.ivf.base64"),
        "rgba" => include_str!("../../../../tests/video-codec-fixtures/eight-frames.rgba.base64"),
        _ => panic!("unknown owned video fixture"),
    };
    base64::engine::general_purpose::STANDARD
        .decode(text.split_whitespace().collect::<String>())
        .unwrap()
}
pub(in crate::engine::script) fn packets() -> Vec<Vec<u8>> {
    let bytes = fixture("ivf");
    assert_eq!(&bytes[..4], b"DKIF");
    assert_eq!(&bytes[8..12], b"AV01");
    assert_eq!(u16::from_le_bytes(bytes[12..14].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(bytes[14..16].try_into().unwrap()), 16);
    let mut offset = 32;
    let mut output = Vec::new();
    while offset < bytes.len() {
        let size = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 12;
        output.push(bytes[offset..offset + size].to_vec());
        offset += size;
    }
    assert_eq!(output.len(), 8);
    output
}

pub(in crate::engine::script) fn javascript_cases() -> String {
    format!(
        "const videoPackets={};const videoReference={};",
        serde_json::to_string(&packets()).unwrap(),
        serde_json::to_string(&fixture("rgba")).unwrap()
    )
}

#[test]
#[ignore = "exports original AV1 packets for the offline browser comparison"]
fn export_video_codec_cases() {
    let path = std::path::PathBuf::from(
        std::env::var_os("BREEZE_VIDEO_CASES_OUTPUT")
            .expect("set BREEZE_VIDEO_CASES_OUTPUT to a task file on G:"),
    );
    assert!(path.is_absolute());
    let display = path.to_string_lossy();
    assert!(display.starts_with("G:\\") || display.starts_with("G:/"));
    assert_eq!(path.extension().unwrap(), "js");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, javascript_cases()).unwrap();
}
