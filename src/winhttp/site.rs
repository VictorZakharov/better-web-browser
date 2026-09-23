//! Schemeful site classification shared by cookies and Fetch Metadata.

use crate::navigation::ParsedUrl;

pub(super) fn same_site(source: &ParsedUrl, target: &ParsedUrl) -> bool {
    source.scheme == target.scheme && site_host(&source.host) == site_host(&target.host)
}

fn site_host(host: &str) -> &str {
    psl2::lookup(host)
        .and_then(|domain| domain.registrable_domain())
        .unwrap_or(host)
}
