//! Windows HTTP facade with bounded transport, cookies, and standards-oriented text decoding.

mod client;
mod cookies;
mod ffi;
mod pipeline;
mod text;

pub use client::{HttpClient, HttpResponse, get};
pub use text::{DecodedText, decode_document, decode_text};

#[cfg(test)]
mod tests;
