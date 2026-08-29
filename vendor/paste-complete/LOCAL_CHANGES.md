# Local packaging

This directory contains the release source of `paste-complete` 1.0.15 from crates.io, whose upstream repository is <https://github.com/esrauch/paste>. The published archive has SHA-256 `3a4cf96a295ab7f0ec2334fa557745c0e9526595f6ebabbb129563a98dfc1b2a` and identifies upstream commit `484f4cf45a8c598c7bfc533e9ac0894014ceafd5`. The source is used unchanged as a version-compatible replacement for V8 152.2's dependency on the archived `paste` crate (`RUSTSEC-2024-0436`). Its local package name is `paste` so V8's published manifest resolves the audited fork without source changes.

Repository CI/tests and package-generated metadata are omitted. `Cargo.toml` retains only build-relevant package/library metadata and marks the local copy as unpublished. Source and license content is unchanged apart from trailing-whitespace/final-newline normalization; the upstream MIT and Apache-2.0 license files are preserved.
