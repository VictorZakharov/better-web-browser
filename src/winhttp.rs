//! Windows HTTP facade with bounded transport, cookies, and standards-oriented text decoding.

mod client;
mod cookies;
mod ffi;
mod pipeline;
mod protocols;
mod site;
mod text;
mod websocket;

pub use client::{HttpClient, HttpResponse, get};
pub use cookies::CookieSnapshot;
pub use pipeline::StreamingFetchResponse;
pub(crate) use text::stream::DocumentDecoder;
pub use text::{DecodedText, decode_document, decode_text};
pub use websocket::{WebSocketConnection, WebSocketFrame, WebSocketOpenResult};
#[cfg(test)]
mod tests;
