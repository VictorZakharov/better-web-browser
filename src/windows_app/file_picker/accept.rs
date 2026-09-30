//! Windows dialog filtering and registered filename MIME metadata.

use std::collections::BTreeSet;
use std::ptr::null;
use windows_sys::Win32::System::Registry::{HKEY_CLASSES_ROOT, RRF_RT_REG_SZ, RegGetValueW};
use windows_sys::Win32::UI::Shell::{ASSOCSTR_CONTENTTYPE, AssocQueryStringW};

pub(super) fn filter_pattern(accept: &str) -> Option<String> {
    let mut patterns = BTreeSet::new();
    for token in accept.split(',').map(str::trim) {
        let token = token.to_ascii_lowercase();
        if valid_extension(&token) {
            patterns.insert(format!("*{token}"));
            continue;
        }
        match token.as_str() {
            "image/*" => patterns.extend(IMAGE_PATTERNS.map(str::to_owned)),
            "audio/*" => patterns.extend(AUDIO_PATTERNS.map(str::to_owned)),
            "video/*" => patterns.extend(VIDEO_PATTERNS.map(str::to_owned)),
            _ if valid_mime(&token) => {
                if let Some(extension) = preferred_extension(&token) {
                    patterns.insert(format!("*{extension}"));
                }
            }
            _ => {}
        }
    }
    (!patterns.is_empty()).then(|| patterns.into_iter().collect::<Vec<_>>().join(";"))
}

pub(super) fn mime_for_name(name: &str) -> String {
    let Some((_, extension)) = name.rsplit_once('.') else {
        return String::new();
    };
    let extension = format!(".{extension}").to_ascii_lowercase();
    if !valid_extension(&extension) {
        return String::new();
    }
    let associated = wide(&extension);
    let mut value = [0_u16; 256];
    let mut length = value.len() as u32;
    let result = unsafe {
        AssocQueryStringW(
            0,
            ASSOCSTR_CONTENTTYPE,
            associated.as_ptr(),
            null(),
            value.as_mut_ptr(),
            &mut length,
        )
    };
    if result != 0 {
        return String::new();
    }
    let mime = String::from_utf16(&value[..(length as usize).min(value.len())])
        .unwrap_or_default()
        .trim_end_matches('\0')
        .to_ascii_lowercase();
    if valid_mime(&mime) {
        mime
    } else {
        String::new()
    }
}

fn preferred_extension(mime: &str) -> Option<String> {
    let key = wide(&format!("MIME\\Database\\Content Type\\{mime}"));
    let value_name = wide("Extension");
    let mut value = [0_u16; 64];
    let mut bytes = std::mem::size_of_val(&value) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CLASSES_ROOT,
            key.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if result != 0 {
        return None;
    }
    let end = value.iter().position(|unit| *unit == 0)?;
    let extension = String::from_utf16(&value[..end]).ok()?.to_ascii_lowercase();
    valid_extension(&extension).then_some(extension)
}

fn valid_extension(extension: &str) -> bool {
    let value = extension.strip_prefix('.').unwrap_or_default();
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        && !value.contains("..")
}

fn valid_mime(mime: &str) -> bool {
    let Some((type_, subtype)) = mime.split_once('/') else {
        return false;
    };
    !type_.is_empty()
        && !subtype.is_empty()
        && mime.len() <= 255
        && type_.bytes().chain(subtype.bytes()).all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&byte)
        })
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

const IMAGE_PATTERNS: [&str; 10] = [
    "*.avif", "*.bmp", "*.gif", "*.heic", "*.heif", "*.jpeg", "*.jpg", "*.png", "*.svg", "*.webp",
];
const AUDIO_PATTERNS: [&str; 7] = [
    "*.aac", "*.flac", "*.m4a", "*.mp3", "*.ogg", "*.wav", "*.weba",
];
const VIDEO_PATTERNS: [&str; 8] = [
    "*.avi", "*.m4v", "*.mov", "*.mp4", "*.mpeg", "*.ogv", "*.webm", "*.wmv",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_extensions_are_hints_without_wildcard_injection() {
        assert_eq!(
            filter_pattern(".TXT, .tar.gz, ../bad, .*.exe"),
            Some("*.tar.gz;*.txt".into())
        );
        assert!(filter_pattern("image/*").unwrap().contains("*.png"));
        assert_eq!(filter_pattern("../foo"), None);
    }
}
