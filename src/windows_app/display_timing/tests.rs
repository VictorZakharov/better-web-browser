use super::*;
use windows_sys::Win32::Graphics::Gdi::DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE;

#[test]
fn boosted_laptop_rate_uses_validated_physical_target_mode() {
    let mut path = DISPLAYCONFIG_PATH_INFO {
        flags: DISPLAYCONFIG_PATH_BOOST_REFRESH_RATE | DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE,
        ..Default::default()
    };
    path.targetInfo.id = 7;
    path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
        Numerator: 120,
        Denominator: 1,
    };
    path.targetInfo.Anonymous.modeInfoIdx = (1 << 16) | 0xffff;
    let mut modes = [DISPLAYCONFIG_MODE_INFO::default(); 2];
    modes[1].infoType = DISPLAYCONFIG_MODE_INFO_TYPE_TARGET;
    modes[1].id = 7;
    modes[1]
        .Anonymous
        .targetMode
        .targetVideoSignalInfo
        .vSyncFreq = DISPLAYCONFIG_RATIONAL {
        Numerator: 239_976,
        Denominator: 1_000,
    };
    assert_eq!(path_rate(&path, &modes), RefreshRate::new(239_976, 1_000));
    modes[1].id = 8;
    assert_eq!(path_rate(&path, &modes), RefreshRate::new(120, 1));
    modes[1].id = 7;
    modes[1].infoType = DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE;
    assert_eq!(path_rate(&path, &modes), RefreshRate::new(120, 1));
    path.targetInfo.Anonymous.modeInfoIdx = 0xffffffff;
    assert_eq!(path_rate(&path, &modes), RefreshRate::new(120, 1));
}

#[test]
fn ordinary_monitor_rate_keeps_its_fraction_and_does_not_choose_another_mode() {
    let mut path = DISPLAYCONFIG_PATH_INFO::default();
    path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
        Numerator: 60000,
        Denominator: 1001,
    };
    assert_eq!(path_rate(&path, &[]), RefreshRate::new(60000, 1001));
    path.targetInfo.refreshRate.Denominator = 0;
    assert_eq!(path_rate(&path, &[]), None);
}
