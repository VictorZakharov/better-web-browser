//! Bound Git's path arguments without weakening pinned-fixture verification.
//! The full curated suite exceeds Windows' process command-line budget.
use std::path::Path;

const PATH_ARGUMENT_BUDGET: usize = 8192;

pub(super) fn verify_unmodified(root: &Path, required: &[String]) -> Result<(), String> {
    // Leave ample room for the executable, scoped configuration and checkout.
    if root.as_os_str().to_string_lossy().encode_utf16().count() > 8192 {
        return Err("WPT checkout path exceeds the Git command budget".into());
    }
    for paths in batches(required)? {
        let status = super::git_command(root)
            .args(["diff", "--quiet", "HEAD", "--"])
            .args(paths)
            .status()
            .map_err(|error| format!("verify WPT fixtures with git: {error}"))?;
        if !status.success() {
            return Err("selected WPT fixtures differ from the pinned revision".into());
        }
    }
    Ok(())
}

fn batches(paths: &[String]) -> Result<Vec<&[String]>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut units = 0;
    for (index, path) in paths.iter().enumerate() {
        // Manifest admission permits only ASCII paths, without whitespace or
        // quotes. Include conservative quoting/separator overhead anyway.
        let cost = path.len().saturating_add(3);
        if cost > PATH_ARGUMENT_BUDGET {
            return Err("WPT fixture path exceeds the Git command budget".into());
        }
        if units + cost > PATH_ARGUMENT_BUDGET {
            result.push(&paths[start..index]);
            start = index;
            units = 0;
        }
        units += cost;
    }
    if start < paths.len() {
        result.push(&paths[start..]);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
