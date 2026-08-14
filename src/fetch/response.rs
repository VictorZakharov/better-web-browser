//! Fetch response metadata and filtered response type.

use super::{Body, FetchUrl, HeaderList};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseType {
    Basic,
    Cors,
    Opaque,
    OpaqueRedirect,
}

#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub response_type: ResponseType,
    pub url_list: Vec<FetchUrl>,
    pub status: u16,
    pub headers: HeaderList,
    pub body: Body,
}

impl FetchResponse {
    pub fn is_success(&self) -> bool {
        (200..=299).contains(&self.status)
    }

    pub fn final_url(&self) -> &FetchUrl {
        self.url_list
            .last()
            .expect("a network response always has a URL")
    }

    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type")
    }
}
