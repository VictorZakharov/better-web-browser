//! Native-window display selection, queried on topology/monitor changes, never per frame.

use super::{BrowserState, Hwnd};
use better_web_browser::engine::rendering::RefreshRate;
use windows_sys::Win32::Devices::Display::*;
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER, ERROR_SUCCESS,
};
use windows_sys::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, HMONITOR,
    MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromWindow,
};

// Windows 11 flag from wingdi.h; the pinned bindings omit this newer constant.
const DISPLAYCONFIG_PATH_BOOST_REFRESH_RATE: u32 = 0x10;

#[derive(Default)]
pub(super) struct DisplayTiming {
    monitor: HMONITOR,
    pub(super) rate: RefreshRate,
    source: Source,
}

#[derive(Default, Clone, Copy)]
enum Source {
    DisplayConfig,
    DisplaySettings,
    #[default]
    Fallback,
}

impl DisplayTiming {
    pub(super) unsafe fn refresh(&mut self, window: Hwnd, force: bool) -> bool {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        if !force && monitor == self.monitor {
            return false;
        }
        self.monitor = monitor;
        let (next, source) =
            monitor_rate(monitor).unwrap_or((RefreshRate::default(), Source::Fallback));
        let changed = next != self.rate;
        self.rate = next;
        self.source = source;
        changed
    }

    pub(super) fn report_json(&self) -> String {
        let source = match self.source {
            Source::DisplayConfig => "display-config",
            Source::DisplaySettings => "display-settings",
            Source::Fallback => "fallback",
        };
        serde_json::json!({"numerator":self.rate.numerator(), "denominator":self.rate.denominator(),
            "source":source, "scope":"animation scheduling frequency; excludes GPU completion and screen presentation"}).to_string()
    }
}

impl BrowserState {
    pub(super) unsafe fn refresh_display_timing(&mut self, force: bool) {
        if self.display_timing.refresh(self.window, force)
            && let (Some(document), Some(session)) = (
                self.navigation.active_document(),
                self.renderer_session.as_ref(),
            )
        {
            // This metadata update has no pending layout/paint transaction. Its
            // runtime reply retimes the wakeup without keeping a static page busy.
            if let Err(error) = session.update_viewport(document, self.renderer_viewport()) {
                self.set_status(&format!("Could not update display cadence: {error}"));
            }
        }
    }
}

unsafe fn monitor_rate(monitor: HMONITOR) -> Option<(RefreshRate, Source)> {
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(monitor, &mut info.monitorInfo) == 0 {
        return None;
    }
    // QueryDisplayConfig preserves fractional frequencies and matches the actual
    // window's source device. DWM's global timing can describe a different monitor.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-querydisplayconfig
    for flags in [
        QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE,
        QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE,
        QDC_ONLY_ACTIVE_PATHS,
    ] {
        match active_rate(flags, &info.szDevice) {
            Ok(Some(rate)) => return Some((rate, Source::DisplayConfig)),
            Err(ERROR_INVALID_PARAMETER) => continue,
            _ => break,
        }
    }
    let mut mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    if EnumDisplaySettingsW(info.szDevice.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) != 0
        && mode.dmDisplayFrequency > 1
    {
        RefreshRate::new(mode.dmDisplayFrequency, 1).map(|rate| (rate, Source::DisplaySettings))
    } else {
        None
    }
}

unsafe fn active_rate(flags: u32, device: &[u16; 32]) -> Result<Option<RefreshRate>, u32> {
    // Hot-plug can race the size query. Bound both retries and allocations; this
    // is a metadata refresh on the UI thread, not an unbounded enumeration loop.
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0, 0);
        let result = GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count);
        if result != ERROR_SUCCESS {
            return Err(result);
        }
        if path_count == 0 || path_count > 128 || mode_count == 0 || mode_count > 512 {
            return Ok(None);
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let result = QueryDisplayConfig(
            flags,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        );
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != ERROR_SUCCESS {
            return Err(result);
        }
        if path_count as usize > paths.len() || mode_count as usize > modes.len() {
            return Ok(None);
        }
        for path in paths.iter().take(path_count as usize) {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut source.header) == 0
                && device_name(&source.viewGdiDeviceName) == device_name(device)
            {
                return Ok(path_rate(path, &modes[..mode_count as usize]));
            }
        }
        return Ok(None);
    }
    Ok(None)
}

fn path_rate(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Option<RefreshRate> {
    use windows_sys::Win32::Graphics::Gdi::DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE;
    if path.flags & DISPLAYCONFIG_PATH_BOOST_REFRESH_RATE != 0 {
        // Windows 11 DRR exposes a lower virtual rate on the path. Animated
        // content can use the physical target rate; do not cap a 240 Hz panel
        // at its idle virtual cadence. Validate the union's target discriminator.
        // https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_info
        let packed = unsafe { path.targetInfo.Anonymous.modeInfoIdx };
        let index = if path.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0 {
            packed >> 16
        } else {
            packed
        };
        if let Some(mode) = modes.get(index as usize)
            && mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
            && mode.id == path.targetInfo.id
            && mode.adapterId.LowPart == path.targetInfo.adapterId.LowPart
            && mode.adapterId.HighPart == path.targetInfo.adapterId.HighPart
        {
            let signal = unsafe { mode.Anonymous.targetMode.targetVideoSignalInfo };
            if let Some(rate) =
                RefreshRate::new(signal.vSyncFreq.Numerator, signal.vSyncFreq.Denominator)
            {
                return Some(rate);
            }
        }
    }
    RefreshRate::new(
        path.targetInfo.refreshRate.Numerator,
        path.targetInfo.refreshRate.Denominator,
    )
}

#[cfg(test)]
#[path = "display_timing/tests.rs"]
mod tests;

fn device_name(value: &[u16]) -> &[u16] {
    &value[..value
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(value.len())]
}
