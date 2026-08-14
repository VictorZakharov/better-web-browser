//! Windows HTTP facade with bounded transport, cookies, and standards-oriented text decoding.

mod client;
mod cookies;
mod ffi;
mod text;

pub use client::{HttpClient, HttpResponse, get};
pub use text::decode_text;

#[cfg(test)]
use crate::navigation::ParsedUrl;
#[cfg(test)]
use cookies::{cookie_matches, parse_cookie};
#[cfg(test)]
use ffi::{ACCEPT_TYPES, WINHTTP_ACCESS_TYPE_NO_PROXY};

#[cfg(test)]
mod tests;
