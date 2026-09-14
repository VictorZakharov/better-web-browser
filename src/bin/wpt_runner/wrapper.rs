//! Window wrappers for unchanged upstream JavaScript tests and their META scripts.
use std::path::Path;
use url::Url;

pub(crate) fn load(root: &Path, test_path: &str) -> Result<String, String> {
    let source = std::fs::read_to_string(root.join(test_path))
        .map_err(|error| format!("read WPT wrapper source {test_path}: {error}"))?;
    html(test_path, &source)
}

fn html(test_path: &str, source: &str) -> Result<String, String> {
    let base = Url::parse(&format!("http://wpt.invalid/{test_path}"))
        .map_err(|error| format!("invalid WPT test path: {error}"))?;
    let mut scripts = String::new();
    for line in source.lines() {
        let Some(reference) = line.trim().strip_prefix("// META: script=") else {
            continue;
        };
        let resolved = base
            .join(reference.trim())
            .map_err(|error| format!("invalid WPT META script: {error}"))?;
        if resolved.origin() != base.origin() {
            return Err("WPT META scripts must remain on the local fixture server".into());
        }
        let path = &resolved[url::Position::BeforePath..];
        scripts.push_str(&format!("<script src=\"{}\"></script>", escape(path)));
    }
    let path = escape(&format!("/{test_path}"));
    Ok(format!(
        "<!doctype html><meta charset=utf-8><base href=\"{path}\"><title>{path}</title>\
         <script src=/resources/testharness.js></script>\
         <script src=/resources/testharnessreport.js></script>{scripts}\
         <script src=\"{path}\"></script><div id=log></div>"
    ))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_relative_and_root_meta_scripts_in_declared_order_before_test() {
        let result = html(
            "user-timing/mark.any.js",
            "// META: global=window,worker\n\
            // META: script=resources/helper.js\n// META: script=/resources/other.js?q=1&x=2",
        )
        .unwrap();
        let helper = result
            .find("src=\"/user-timing/resources/helper.js\"")
            .unwrap();
        let other = result
            .find("src=\"/resources/other.js?q=1&amp;x=2\"")
            .unwrap();
        let test = result.find("src=\"/user-timing/mark.any.js\"").unwrap();
        assert!(helper < other && other < test);
        assert!(result.contains("<base href=\"/user-timing/mark.any.js\">"));
    }

    #[test]
    fn refuses_external_dependency_requests() {
        assert!(
            html(
                "test.any.js",
                "// META: script=https://example.com/remote.js"
            )
            .is_err()
        );
        assert!(html("test.any.js", "// META: script=//example.com/remote.js").is_err());
    }
}
