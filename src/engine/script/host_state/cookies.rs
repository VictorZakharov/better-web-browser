//! Script-visible cookie projection kept in sync with the network cookie store.

use super::*;

impl HostState {
    pub(in crate::engine::script) fn cookie_header(&self) -> String {
        let mut cookies = self.cookies.iter().collect::<Vec<_>>();
        cookies.sort_unstable_by(|left, right| left.0.cmp(right.0));
        cookies
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub(in crate::engine::script) fn replace_cookies_from_header(&mut self, cookie_header: &str) {
        self.cookies.clear();
        for pair in cookie_header.split(';').map(str::trim) {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if !name.is_empty() {
                self.cookies
                    .insert(name.to_string(), value.trim().to_string());
            }
        }
    }

    pub(in crate::engine::script) fn set_cookie(&mut self, assignment: String) {
        let Some(pair) = assignment.split(';').next().map(str::trim) else {
            return;
        };
        let Some((name, value)) = pair.split_once('=') else {
            return;
        };
        let name = name.trim();
        if name.is_empty() || name.bytes().any(|byte| byte <= 0x20 || byte == b';') {
            return;
        }

        let expired = assignment.split(';').skip(1).any(|attribute| {
            attribute
                .trim()
                .split_once('=')
                .is_some_and(|(name, value)| {
                    name.trim().eq_ignore_ascii_case("max-age")
                        && value
                            .trim()
                            .parse::<i64>()
                            .is_ok_and(|seconds| seconds <= 0)
                })
        });
        if expired {
            self.cookies.remove(name);
        } else {
            self.cookies
                .insert(name.to_string(), value.trim().to_string());
        }
        self.cookie_updates.push(assignment);
    }
}
