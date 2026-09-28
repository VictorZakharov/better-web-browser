//! CacheQueryOptions and Vary matching over the stored request/response pair.
use super::{CacheEntry, CacheQueryOptions, CacheRequest};
use url::Url;

pub(super) fn matches_entry(
    query: &CacheRequest,
    entry: &CacheEntry,
    options: CacheQueryOptions,
) -> bool {
    if !options.ignore_method && query.method != "GET" {
        return false;
    }
    let Ok(mut query_url) = Url::parse(&query.url) else {
        return false;
    };
    let Ok(mut cached_url) = Url::parse(&entry.request.url) else {
        return false;
    };
    query_url.set_fragment(None);
    cached_url.set_fragment(None);
    if options.ignore_search {
        query_url.set_query(None);
        cached_url.set_query(None);
    }
    if query_url != cached_url {
        return false;
    }
    if options.ignore_vary {
        return true;
    }
    vary_fields(&entry.response.headers)
        .into_iter()
        .all(|field| {
            field != "*"
                && combined_header(&entry.request.headers, &field)
                    == combined_header(&query.headers, &field)
        })
}

pub(super) fn vary_fields(headers: &[(String, String)]) -> Vec<String> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("vary"))
        .flat_map(|(_, value)| value.split(','))
        .map(|field| field.trim().to_ascii_lowercase())
        .filter(|field| !field.is_empty())
        .collect()
}

fn combined_header(headers: &[(String, String)], name: &str) -> String {
    headers
        .iter()
        .filter(|(field, _)| field.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
