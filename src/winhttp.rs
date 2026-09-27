//! Windows HTTP facade with bounded transport, cookies, and standards-oriented text decoding.

mod client;
mod cookies;
mod ffi;
mod pipeline;
mod protocols;
mod site;
mod websocket;

pub(crate) use crate::text_decode::DocumentDecoder;
pub use crate::text_decode::{DecodedText, decode_document, decode_text};
pub use client::{HttpClient, HttpResponse, get};
pub use cookies::CookieSnapshot;
pub use pipeline::StreamingFetchResponse;
pub use websocket::{WebSocketConnection, WebSocketFrame, WebSocketOpenResult};
#[cfg(test)]
mod tests;
