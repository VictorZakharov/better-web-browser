//! Native failure injection verifies ordering without assuming a CI GPU exists.
use super::*;

#[test]
fn hardware_success_does_not_create_a_software_peer() {
    let mut attempted = Vec::new();
    let result = admit(Options::default(), false, None, |backend| {
        attempted.push(backend);
        Ok(backend)
    })
    .unwrap();
    assert_eq!(result, Backend::Hardware(None));
    assert_eq!(attempted, vec![result]);
}

#[test]
fn preferred_adapter_failure_uses_default_hardware_before_software() {
    let id = AdapterId {
        high: -1,
        low: u32::MAX,
    };
    let mut attempted = Vec::new();
    let result = admit(Options::default(), false, Some(id), |backend| {
        attempted.push(backend);
        match backend {
            Backend::Hardware(Some(_)) => Err("adapter unavailable".into()),
            _ => Ok(backend),
        }
    })
    .unwrap();
    assert_eq!(result, Backend::Hardware(None));
    assert_eq!(
        attempted,
        vec![Backend::Hardware(Some(id)), Backend::Hardware(None)]
    );
}

#[test]
fn fallback_covers_complete_admission_failure_not_just_egl_display_creation() {
    for failure in [
        "GLES version",
        "core entry point",
        "private surface",
        "native limits",
    ] {
        let mut attempted = Vec::new();
        let result = admit(Options::default(), false, None, |backend| {
            attempted.push(backend);
            match backend {
                Backend::Hardware(_) => Err(failure.into()),
                Backend::Software => Ok(backend),
            }
        })
        .unwrap();
        assert_eq!(result, Backend::Software);
        assert_eq!(attempted, vec![Backend::Hardware(None), Backend::Software]);
    }
}

#[test]
fn strict_caveat_refuses_software_even_after_a_real_hardware_failure() {
    let options = Options {
        fail_if_major_performance_caveat: true,
        ..Options::default()
    };
    let mut attempted = Vec::new();
    let result = admit(options, false, None, |backend| {
        attempted.push(backend);
        Err::<(), _>("hardware unavailable".into())
    })
    .unwrap_err();
    assert!(result.contains("hardware unavailable"));
    assert!(result.contains("failIfMajorPerformanceCaveat"));
    assert_eq!(attempted, vec![Backend::Hardware(None)]);
    assert!(admit(options, true, None, |_| Ok(())).is_err());
}

#[test]
fn strict_caveat_allows_a_real_hardware_context_and_keeps_the_hint_optional() {
    let options = Options {
        fail_if_major_performance_caveat: true,
        ..Options::default()
    };
    assert_eq!(
        admit(options, false, None, Ok).unwrap(),
        Backend::Hardware(None)
    );
}

#[test]
fn software_only_tests_do_not_enumerate_or_create_hardware() {
    let mut attempted = Vec::new();
    let result = admit(
        Options::default(),
        true,
        Some(AdapterId { high: 1, low: 2 }),
        |backend| {
            attempted.push(backend);
            Ok(backend)
        },
    )
    .unwrap();
    assert_eq!(result, Backend::Software);
    assert_eq!(attempted, vec![Backend::Software]);
}

#[test]
fn failure_diagnostics_keep_both_backend_errors() {
    let reason = admit(Options::default(), false, None, |backend| {
        Err::<(), _>(format!("{backend:?} driver failure"))
    })
    .unwrap_err();
    assert!(reason.contains("default hardware: Hardware(None) driver failure"));
    assert!(reason.contains("software fallback: Software driver failure"));
}

#[test]
fn egl_attributes_select_only_real_renderers_and_preserve_luid_bits() {
    assert_eq!(
        Backend::Software.attributes(),
        vec![0x3203, 0x3208, 0x3209, 0x320b, 0x3451, 0, 0x3038]
    );
    assert_eq!(
        Backend::Hardware(None).attributes(),
        vec![0x3203, 0x3208, 0x3209, 0x320a, 0x3451, 0, 0x3038]
    );
    assert_eq!(
        Backend::Hardware(Some(AdapterId {
            high: i32::MIN,
            low: u32::MAX
        }))
        .attributes(),
        vec![
            0x3203,
            0x3208,
            0x3209,
            0x320a,
            0x3451,
            0,
            0x34a0,
            i32::MIN,
            0x34a1,
            -1,
            0x3038
        ]
    );
}
