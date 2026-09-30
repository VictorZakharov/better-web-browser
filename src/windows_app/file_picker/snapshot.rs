//! Immutable, bounded snapshots taken only after a browser-owned picker closes.
//!
//! File API exposes basename, lowercase MIME (or empty), and modified milliseconds.
//! https://w3c.github.io/FileAPI/#dfn-file

use super::accept;
use better_web_browser::renderer_protocol::{
    MAX_FILE_PICKER_BYTES, MAX_FILE_PICKER_FILES, MAX_FILE_PICKER_NAME_BYTES, SelectedFileMetadata,
};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

pub(super) struct SelectedFile {
    pub metadata: SelectedFileMetadata,
    pub bytes: Vec<u8>,
}

pub(super) fn read_selected(paths: &[PathBuf], multiple: bool) -> Result<Vec<SelectedFile>, ()> {
    if paths.is_empty() || paths.len() > MAX_FILE_PICKER_FILES || (!multiple && paths.len() != 1) {
        return Err(());
    }
    let mut remaining = MAX_FILE_PICKER_BYTES;
    let mut selected = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path.file_name().and_then(|part| part.to_str()).ok_or(())?;
        if name.is_empty() || name.len() > MAX_FILE_PICKER_NAME_BYTES {
            return Err(());
        }
        let mut file = File::open(path).map_err(|_| ())?;
        let file_metadata = file.metadata().map_err(|_| ())?;
        if !file_metadata.is_file() || file_metadata.len() > remaining as u64 {
            return Err(());
        }
        let expected_size = file_metadata.len() as usize;
        let mut bytes = Vec::with_capacity(expected_size);
        file.by_ref()
            .take(remaining as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        // A concurrent size change is rejected; no partial selection is installed.
        if bytes.len() != expected_size {
            return Err(());
        }
        remaining = remaining.checked_sub(bytes.len()).ok_or(())?;
        let metadata = SelectedFileMetadata {
            name: name.to_owned(),
            mime_type: accept::mime_for_name(name),
            last_modified: unix_millis(
                file_metadata
                    .modified()
                    .unwrap_or_else(|_| SystemTime::now()),
            ),
            size: bytes.len() as u32,
        };
        metadata.validate().map_err(|_| ())?;
        selected.push(SelectedFile { metadata, bytes });
    }
    Ok(selected)
}

fn unix_millis(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(MAX_SAFE_INTEGER as u128) as i64,
        Err(error) => -(error.duration().as_millis().min(MAX_SAFE_INTEGER as u128) as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::file_picker::FilePickerBackend;
    use std::fs;

    struct FakePicker(Vec<PathBuf>);

    impl FilePickerBackend for FakePicker {
        fn select(
            &self,
            _owner: crate::windows_app::platform::Hwnd,
            _multiple: bool,
            _accept: &str,
        ) -> Result<Option<Vec<PathBuf>>, ()> {
            Ok(Some(self.0.clone()))
        }
    }

    #[test]
    fn fake_picker_snapshots_bytes_without_native_ui() {
        let directory = std::env::temp_dir().join(format!("bwb-picker-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("upload.txt");
        fs::write(&path, b"bounded snapshot").unwrap();
        let fake = FakePicker(vec![path.clone()]);
        let paths = fake
            .select(std::ptr::null_mut(), false, ".txt")
            .unwrap()
            .unwrap();
        let files = read_selected(&paths, false).unwrap();
        assert_eq!(files[0].metadata.name, "upload.txt");
        assert_eq!(files[0].bytes, b"bounded snapshot");
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn selection_limits_are_checked_before_opening_files() {
        let paths = vec![PathBuf::from("missing"); 9];
        assert!(read_selected(&paths, true).is_err());
        assert!(read_selected(&paths[..2], false).is_err());
    }
}
