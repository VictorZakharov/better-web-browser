//! Bounded DNS-only origin warmup for HTML resource hints.
//!
//! WinHttpConnect creates a WinHTTP handle but does not guarantee a TCP/TLS handshake or a
//! connection available to another request. Resolving through the OS resolver is the honest
//! partial preconnect implementation allowed by HTML; it also benefits dns-prefetch. We do not
//! issue a HEAD/GET to simulate a handshake because that would be an observable extra request.
//! `ToSocketAddrs` uses the platform resolver's timeout rather than a cancelable deadline, so
//! the document and browser process both limit this best-effort work to two background Fetch
//! slots. Browser UI and renderer execution never wait for the resolver.
//! https://html.spec.whatwg.org/multipage/links.html#link-type-preconnect

use super::HttpClient;
use crate::fetch::{FetchError, FetchSignal};
use crate::navigation::ParsedUrl;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_INFLIGHT_DNS_HINTS: usize = 2;
static INFLIGHT_DNS_HINTS: AtomicUsize = AtomicUsize::new(0);

struct DnsHintPermit<'a>(&'a AtomicUsize);

impl<'a> DnsHintPermit<'a> {
    fn try_acquire(counter: &'a AtomicUsize) -> Option<Self> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_INFLIGHT_DNS_HINTS).then_some(active + 1)
            })
            .ok()
            .map(|_| Self(counter))
    }
}

impl Drop for DnsHintPermit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

impl HttpClient {
    pub fn warm_origin_dns(&self, origin: &str, signal: &FetchSignal) -> Result<(), FetchError> {
        signal.check()?;
        let parsed =
            ParsedUrl::parse(origin).map_err(|error| FetchError::network(error.to_string()))?;
        // OS DNS has no cancellation hook here. Skip optional work when another document
        // already occupies both browser-wide slots, rather than accumulating blocked threads.
        let Some(_permit) = DnsHintPermit::try_acquire(&INFLIGHT_DNS_HINTS) else {
            return Ok(());
        };
        signal.check()?;
        let mut addresses = (parsed.host.as_str(), parsed.port)
            .to_socket_addrs()
            .map_err(|error| FetchError::network(format!("resolve hint origin: {error}")))?;
        if addresses.next().is_none() {
            return Err(FetchError::network("hint origin resolved to no addresses"));
        }
        signal.check()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn dns_hint_concurrency_is_bounded_across_batches() {
        let counter = AtomicUsize::new(0);
        let first = DnsHintPermit::try_acquire(&counter).unwrap();
        let second = DnsHintPermit::try_acquire(&counter).unwrap();
        assert!(DnsHintPermit::try_acquire(&counter).is_none());
        drop(first);
        assert!(DnsHintPermit::try_acquire(&counter).is_some());
        drop(second);
    }

    #[test]
    fn warmup_resolves_localhost_without_opening_an_http_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = HttpClient::new().unwrap();
        client
            .warm_origin_dns(&format!("http://localhost:{port}"), &FetchSignal::default())
            .unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn warmup_rejects_non_http_and_credentialed_urls() {
        let client = HttpClient::new().unwrap();
        for url in ["data:text/plain,hello", "https://user:pass@localhost/"] {
            assert!(
                client
                    .warm_origin_dns(url, &FetchSignal::default())
                    .is_err()
            );
        }
    }
}
