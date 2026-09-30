//! STA Common Item Dialog adapter. Only browser-owned paths cross this boundary.

use super::{FilePickerBackend, accept};
use crate::windows_app::platform::Hwnd;
use better_web_browser::renderer_protocol::MAX_FILE_PICKER_FILES;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_ALLOWMULTISELECT, FOS_DONTADDTORECENT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
    FOS_NODEREFERENCELINKS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};
use windows::core::{PCWSTR, w};

pub(in crate::windows_app) struct NativeFilePicker;

impl FilePickerBackend for NativeFilePicker {
    fn select(
        &self,
        owner: Hwnd,
        multiple: bool,
        accept: &str,
    ) -> Result<Option<Vec<PathBuf>>, ()> {
        // The browser message loop initialized an STA before any HWND was created.
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| ())?;
        let mut options = unsafe { dialog.GetOptions() }.map_err(|_| ())?
            | FOS_FORCEFILESYSTEM
            | FOS_FILEMUSTEXIST
            | FOS_NODEREFERENCELINKS
            | FOS_DONTADDTORECENT;
        if multiple {
            options |= FOS_ALLOWMULTISELECT;
        }
        unsafe { dialog.SetOptions(options) }.map_err(|_| ())?;
        if let Some(pattern) = accept::filter_pattern(accept) {
            let wide_pattern = pattern.encode_utf16().chain([0]).collect::<Vec<_>>();
            let filters = [
                COMDLG_FILTERSPEC {
                    pszName: w!("Accepted files"),
                    pszSpec: PCWSTR(wide_pattern.as_ptr()),
                },
                COMDLG_FILTERSPEC {
                    pszName: w!("All files"),
                    pszSpec: w!("*.*"),
                },
            ];
            // `accept` is a hint: the user may choose the All files entry.
            unsafe { dialog.SetFileTypes(&filters) }.map_err(|_| ())?;
        }
        match unsafe { dialog.Show(Some(HWND(owner))) } {
            Ok(()) => {}
            // HRESULT_FROM_WIN32(ERROR_CANCELLED).
            Err(error) if error.code().0 as u32 == 0x8007_04c7 => return Ok(None),
            Err(_) => return Err(()),
        }
        let selected = unsafe { dialog.GetResults() }.map_err(|_| ())?;
        let count = unsafe { selected.GetCount() }.map_err(|_| ())?;
        if count == 0 || count as usize > MAX_FILE_PICKER_FILES || (!multiple && count != 1) {
            return Err(());
        }
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let item = unsafe { selected.GetItemAt(index) }.map_err(|_| ())?;
            let pointer = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|_| ())?;
            if pointer.is_null() {
                return Err(());
            }
            let path = PathBuf::from(OsString::from_wide(unsafe { pointer.as_wide() }));
            unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
            paths.push(path);
        }
        Ok(Some(paths))
    }
}
