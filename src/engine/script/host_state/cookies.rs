//! Script-visible cookie projection kept in sync with the network cookie store.

use super::*;

impl HostState {
    pub(in crate::engine::script) fn cookie_header(&self) -> String {
        self.cookie_header.clone()
    }

    pub(in crate::engine::script) fn replace_cookies_from_header(&mut self, cookie_header: &str) {
        self.cookie_header.clear();
        self.cookie_header.push_str(cookie_header);
    }

    pub(in crate::engine::script) fn replace_cookie_snapshot(
        &mut self,
        version: u64,
        cookie_header: &str,
    ) {
        self.cookie_version = version;
        self.replace_cookies_from_header(cookie_header);
        self.cookie_updates.clear();
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
        let mut pairs = self
            .cookie_header
            .split(';')
            .map(str::trim)
            .filter(|pair| !pair.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();

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
            pairs.retain(|pair| cookie_pair_name(pair) != Some(name));
        } else {
            let serialized = format!("{name}={}", value.trim());
            if let Some(pair) = pairs
                .iter_mut()
                .find(|pair| cookie_pair_name(pair) == Some(name))
            {
                *pair = serialized;
            } else {
                pairs.push(serialized);
            }
        }
        self.cookie_header = pairs.join("; ");
        self.cookie_version = self.cookie_version.checked_add(1).unwrap_or(1);
        self.cookie_updates.push(assignment);
    }
}

fn cookie_pair_name(pair: &str) -> Option<&str> {
    pair.split_once('=').map(|(name, _)| name.trim())
}
