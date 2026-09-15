//! Literal Content-Type sidecars used by upstream MIME tests. Unsupported WPT
//! substitution/header directives fail closed instead of silently changing the test.
use std::path::Path;

pub(super) fn content_type(root: &Path, fixture: &Path) -> Result<Option<String>, String> {
    let mut path = fixture.as_os_str().to_os_string();
    path.push(".headers");
    let path = Path::new(&path);
    if !path.exists() {
        return Ok(None);
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) || std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 8192
    {
        return Err("unsafe or oversized WPT header sidecar".into());
    }
    parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
}

fn parse(source: &str) -> Result<Option<String>, String> {
    let mut result = None;
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, value)) = line.split_once(':') else {
            return Err("invalid WPT header sidecar".into());
        };
        let value = value.trim();
        if !name.eq_ignore_ascii_case("Content-Type")
            || value.is_empty()
            || value.bytes().any(|b| !(32..=126).contains(&b))
            || value.contains("{{")
        {
            return Err("unsupported WPT header sidecar directive".into());
        }
        if result.replace(value.to_string()).is_some() {
            return Err("duplicate WPT Content-Type header".into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_mime_sidecars_are_honored_and_unsupported_directives_are_rejected() {
        assert_eq!(
            parse("Content-Type: text/html\r\n").unwrap().as_deref(),
            Some("text/html")
        );
        assert!(parse("Content-Type: {{value}}").is_err());
        assert!(parse("Content-Type: text/css\nContent-Type: text/html").is_err());
        assert!(parse("Access-Control-Allow-Origin: *").is_err());
    }
}
