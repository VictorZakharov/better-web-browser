use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Upstream {
    pub(crate) repository: String,
    pub(crate) revision: String,
    pub(crate) license: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExpectationsPolicy {
    Allowed,
    Forbidden,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Policy {
    pub(crate) minimum_subtests: usize,
    pub(crate) expectations: ExpectationsPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExpectedStatus {
    Pass,
    Fail,
    Timeout,
}

impl std::fmt::Display for ExpectedStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
        };
        formatter.write_str(name)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TestCase {
    pub(crate) path: String,
    pub(crate) area: String,
    pub(crate) expected: ExpectedStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

impl TestCase {
    pub(crate) fn needs_wrapper(&self) -> bool {
        self.path.ends_with(".js")
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) schema: u32,
    pub(crate) suite: String,
    pub(crate) upstream: Upstream,
    pub(crate) policy: Policy,
    #[serde(default)]
    pub(crate) support: Vec<String>,
    pub(crate) tests: Vec<TestCase>,
}

impl Manifest {
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("read manifest {}: {error}", path.display()))?;
        let manifest: Self = serde_json::from_str(&source)
            .map_err(|error| format!("parse manifest {}: {error}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn selected(&self, filter: Option<&str>) -> Result<Vec<TestCase>, String> {
        let Some(filter) = filter else {
            return Ok(self.tests.clone());
        };
        let needle = filter.to_ascii_lowercase();
        let tests: Vec<_> = self
            .tests
            .iter()
            .filter(|test| {
                test.path.to_ascii_lowercase().contains(&needle)
                    || test.area.to_ascii_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        if tests.is_empty() {
            return Err(format!("no curated WPT case matches filter {filter:?}"));
        }
        Ok(tests)
    }

    pub(crate) fn verify_checkout(
        &self,
        root: &Path,
        tests: &[TestCase],
    ) -> Result<PathBuf, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("open WPT checkout {}: {error}", root.display()))?;
        let revision = git_output(&root, &["rev-parse", "HEAD"])?;
        if revision != self.upstream.revision {
            return Err(format!(
                "WPT checkout is at {revision}, but the manifest requires {}",
                self.upstream.revision
            ));
        }

        let mut required = vec!["resources/testharness.js".to_string()];
        required.extend(self.support.iter().cloned());
        required.extend(tests.iter().map(|test| test.path.clone()));
        required.sort();
        required.dedup();
        for relative in &required {
            let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !path.is_file() {
                return Err(format!(
                    "required WPT fixture is missing: {relative}; run scripts/checkout-wpt.ps1"
                ));
            }
        }

        let mut diff = git_command(&root);
        diff.args(["diff", "--quiet", "HEAD", "--"]);
        for path in required {
            diff.arg(path);
        }
        let status = diff
            .status()
            .map_err(|error| format!("verify WPT fixtures with git: {error}"))?;
        if !status.success() {
            return Err("selected WPT fixtures differ from the pinned revision".to_string());
        }
        Ok(root)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema != 2 {
            return Err(format!("unsupported WPT manifest schema {}", self.schema));
        }
        if self.suite.trim().is_empty() {
            return Err("manifest suite must not be empty".to_string());
        }
        if self.upstream.repository != "https://github.com/web-platform-tests/wpt.git" {
            return Err("manifest must identify the official WPT repository".to_string());
        }
        if self.upstream.revision.len() != 40
            || !self
                .upstream
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("manifest revision must be a full 40-character Git hash".to_string());
        }
        if self.tests.is_empty() {
            return Err("manifest must contain at least one test".to_string());
        }
        if self.policy.minimum_subtests == 0 {
            return Err("manifest minimum_subtests must be greater than zero".to_string());
        }

        let mut paths = HashSet::new();
        for support in &self.support {
            validate_fixture_path(support)?;
            if !paths.insert(support) {
                return Err(format!("duplicate WPT path: {support}"));
            }
        }
        for test in &self.tests {
            validate_test(test)?;
            if self.policy.expectations == ExpectationsPolicy::Forbidden
                && test.expected != ExpectedStatus::Pass
            {
                return Err(format!(
                    "manifest policy forbids expectations: {}",
                    test.path
                ));
            }
            if !paths.insert(&test.path) {
                return Err(format!("duplicate WPT path: {}", test.path));
            }
        }
        Ok(())
    }
}

fn validate_fixture_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/-_.".contains(&byte))
    {
        return Err(format!("unsafe or unsupported WPT fixture path: {value}"));
    }
    Ok(())
}

fn validate_test(test: &TestCase) -> Result<(), String> {
    validate_fixture_path(&test.path)?;
    if !(test.path.ends_with(".html") || test.path.ends_with(".htm") || test.path.ends_with(".js"))
    {
        return Err(format!("unsupported WPT test type: {}", test.path));
    }
    if test.area.trim().is_empty() {
        return Err(format!("test area is empty: {}", test.path));
    }
    if test.expected != ExpectedStatus::Pass
        && test
            .reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        return Err(format!("expected failure needs a reason: {}", test.path));
    }
    Ok(())
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = git_command(root)
        .args(arguments)
        .output()
        .map_err(|error| format!("run git in WPT checkout: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git failed in WPT checkout: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    // Scoped safe.directory avoids changing user Git configuration when a CI/sandbox identity
    // verifies a checkout prepared by the host user.
    command
        .arg("-c")
        .arg(format!("safe.directory={}", root.display()))
        .arg("-c")
        .arg(if cfg!(windows) {
            "core.excludesFile=NUL"
        } else {
            "core.excludesFile=/dev/null"
        })
        .arg("-C")
        .arg(root);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(path: &str, expected: ExpectedStatus, reason: Option<&str>) -> TestCase {
        TestCase {
            path: path.to_string(),
            area: "DOM".to_string(),
            expected,
            reason: reason.map(str::to_string),
        }
    }

    #[test]
    fn rejects_traversal_and_unexplained_failures() {
        assert!(validate_test(&case("../secret.html", ExpectedStatus::Pass, None)).is_err());
        assert!(validate_test(&case("dom/test.html", ExpectedStatus::Fail, None)).is_err());
    }

    #[test]
    fn recognizes_javascript_wrappers() {
        assert!(case("url/a.any.js", ExpectedStatus::Pass, None).needs_wrapper());
        assert!(case("fetch/cloned-any.js", ExpectedStatus::Pass, None).needs_wrapper());
        assert!(!case("dom/a.html", ExpectedStatus::Pass, None).needs_wrapper());
    }

    #[test]
    fn curated_policy_rejects_expectations() {
        let manifest = Manifest {
            schema: 2,
            suite: "curated".to_string(),
            upstream: Upstream {
                repository: "https://github.com/web-platform-tests/wpt.git".to_string(),
                revision: "a".repeat(40),
                license: "BSD-3-Clause".to_string(),
            },
            policy: Policy {
                minimum_subtests: 1,
                expectations: ExpectationsPolicy::Forbidden,
            },
            support: Vec::new(),
            tests: vec![case(
                "dom/test.html",
                ExpectedStatus::Fail,
                Some("known gap"),
            )],
        };

        assert!(manifest.validate().is_err());
    }
}
