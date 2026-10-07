use super::*;
use std::path::PathBuf;

fn many_paths() -> Vec<String> {
    (0..300)
        .map(|index| format!("fixture-{index:04}-{}.html", "x".repeat(120)))
        .collect()
}

#[test]
fn large_fixture_lists_are_bounded_without_dropping_the_last_path() {
    let paths = many_paths();
    assert!(paths.iter().map(String::len).sum::<usize>() > 32768);
    let groups = batches(&paths).unwrap();
    assert!(groups.len() > 1);
    for group in &groups {
        assert!(!group.is_empty());
        assert!(group.iter().map(|path| path.len() + 3).sum::<usize>() <= PATH_ARGUMENT_BUDGET);
    }
    assert_eq!(
        groups.into_iter().flatten().collect::<Vec<_>>(),
        paths.iter().collect::<Vec<_>>()
    );
    assert!(batches(&[]).unwrap().is_empty());
    assert!(batches(&["x".repeat(PATH_ARGUMENT_BUDGET)]).is_err());
}

struct Checkout(PathBuf);

impl Drop for Checkout {
    fn drop(&mut self) {
        // Only remove the exact unique temporary checkout created by this test.
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("breeze-wpt-git-")
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn git_verification_rejects_a_modified_fixture_in_the_final_batch() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = Checkout(
        std::env::temp_dir().join(format!("breeze-wpt-git-{}-{unique}", std::process::id())),
    );
    std::fs::create_dir(&root.0).unwrap();
    let paths = many_paths();
    for path in &paths {
        std::fs::write(root.0.join(path), "unchanged\n").unwrap();
    }
    let git = |args: &[&str]| {
        let output = crate::manifest::git_command(&root.0)
            .args(["-c", "core.excludesFile="])
            .arg("-c")
            .arg(format!(
                "core.hooksPath={}",
                root.0.join("no-hooks").display()
            ))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--initial-branch=fixture"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Breeze test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "Pinned fixture test",
    ]);
    verify_unmodified(&root.0, &paths).unwrap();
    std::fs::write(root.0.join(paths.last().unwrap()), "changed\n").unwrap();
    assert!(verify_unmodified(&root.0, &paths).is_err());
    // A change outside the selected set remains irrelevant, as before.
    verify_unmodified(&root.0, &paths[..1]).unwrap();
}
