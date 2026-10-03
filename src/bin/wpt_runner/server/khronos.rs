//! Attach only result reporting; upstream test/support bytes remain unchanged on disk.
use crate::manifest::{HarnessKind, TestCase};
const REPORTER: &str = include_str!("../../../../tests/webgl/khronos-reporter.js");

pub(super) fn adapt(tests: &[TestCase], path: &str, body: Vec<u8>) -> Result<Vec<u8>, String> {
    let Some(test) = tests.iter().find(|test| {
        path.strip_prefix('/') == Some(test.path.as_str()) && test.harness == HarnessKind::Khronos
    }) else {
        return Ok(body);
    };
    let source = String::from_utf8(body).map_err(|_| "Khronos fixture is not UTF-8")?;
    let position = source
        .to_ascii_lowercase()
        .find("<head>")
        .ok_or("Khronos fixture has no head insertion point")?
        + "<head>".len();
    // Manifest validation limits this name to ASCII identifier characters. JSON
    // quoting still makes the reporting configuration unambiguous and non-executable.
    let extension =
        serde_json::to_string(&test.required_extension).map_err(|error| error.to_string())?;
    let adapter = format!(
        "<script>globalThis.__breezeRequiredWebGlExtension={extension};\n{REPORTER}</script>"
    );
    let mut html = String::with_capacity(source.len() + adapter.len());
    html.push_str(&source[..position]);
    html.push_str(&adapter);
    html.push_str(&source[position..]);
    Ok(html.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ExpectedStatus;

    fn case(path: &str) -> TestCase {
        TestCase {
            path: path.into(),
            area: "WebGL".into(),
            expected: ExpectedStatus::Pass,
            reason: None,
            harness: HarnessKind::Khronos,
            required_extension: Some("ANGLE_instanced_arrays".into()),
        }
    }

    #[test]
    fn adapter_adds_reporting_but_preserves_every_original_fixture_byte() {
        let before = "<!doctype html><HEAD>\n<title>shader 🎨</title></HEAD><body><script>testPassed('real assertion');</script>";
        let result = String::from_utf8(
            adapt(
                &[case("test.html")],
                "/test.html",
                before.as_bytes().to_vec(),
            )
            .unwrap(),
        )
        .unwrap();
        let (prefix, suffix) = before.split_at(before.find("<HEAD>").unwrap() + 6);
        assert!(result.starts_with(prefix));
        assert!(result.ends_with(suffix));
        assert!(result.contains("__breezeRequiredWebGlExtension=\"ANGLE_instanced_arrays\""));
        assert!(result.contains(REPORTER));
        assert_eq!(result.matches("testPassed('real assertion')").count(), 1);
    }

    #[test]
    fn unrelated_support_files_and_wpt_cases_are_not_rewritten() {
        let source = b"arbitrary binary \xff\x00".to_vec();
        assert_eq!(
            adapt(&[case("test.html")], "/support.js", source.clone()).unwrap(),
            source
        );
        let mut wpt = case("test.html");
        wpt.harness = HarnessKind::Testharness;
        assert_eq!(adapt(&[wpt], "/test.html", source.clone()).unwrap(), source);
    }

    #[test]
    fn malformed_selected_fixtures_fail_closed() {
        assert!(adapt(&[case("test.html")], "/test.html", vec![0xff]).is_err());
        assert!(
            adapt(
                &[case("test.html")],
                "/test.html",
                b"<body>not a fixture".to_vec()
            )
            .is_err()
        );
    }
}
