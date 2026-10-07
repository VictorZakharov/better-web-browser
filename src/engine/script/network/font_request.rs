//! CSS font fetching uses Fetch's font destination, not an author fetch's empty
//! destination. Preserve the existing transport/CORS/redirect machinery while
//! selecting font-src and font cache/preload policy at the browser boundary.
//! https://drafts.csswg.org/css-fonts-4/#font-fetching-requirements
use super::*;

pub(in crate::engine::script) fn prepare(request: &mut FetchRequest) -> JsResult<()> {
    if request.method != "GET"
        || request.body.is_some()
        || request.keepalive
        || request.mode != RequestMode::Cors
        || request.credentials != CredentialsMode::SameOrigin
        || request.redirect != RedirectMode::Follow
        || request.cache != RequestCache::Default
        || !request.headers.is_empty()
    {
        return Err(type_error("invalid private font request"));
    }
    request.context = crate::fetch::RequestContext::Subresource;
    request.destination = crate::fetch::RequestDestination::Font;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinary() -> FetchRequest {
        let mut request =
            FetchRequest::script("https://example.test/font.ttf", "https://example.test/").unwrap();
        request.mode = RequestMode::Cors;
        request
    }

    #[test]
    fn ordinary_anonymous_cors_font_keeps_its_origin_and_referrer() {
        let mut request = ordinary();
        let origin = request.origin.clone();
        let referrer = request.referrer.clone();
        prepare(&mut request).unwrap();
        assert_eq!(request.destination, crate::fetch::RequestDestination::Font);
        assert_eq!(request.context, crate::fetch::RequestContext::Subresource);
        assert_eq!(request.origin, origin);
        assert_eq!(request.referrer, referrer);
    }

    #[test]
    fn private_font_admission_rejects_non_font_request_semantics_atomically() {
        let mut variants = Vec::new();
        let mut request = ordinary();
        request.method = "POST".into();
        variants.push(request);
        let mut request = ordinary();
        request.body = Some(Body::from_bytes(vec![1]));
        variants.push(request);
        let mut request = ordinary();
        request.keepalive = true;
        variants.push(request);
        let mut request = ordinary();
        request.mode = RequestMode::NoCors;
        variants.push(request);
        let mut request = ordinary();
        request.credentials = CredentialsMode::Include;
        variants.push(request);
        let mut request = ordinary();
        request.redirect = RedirectMode::Manual;
        variants.push(request);
        let mut request = ordinary();
        request.cache = RequestCache::NoStore;
        variants.push(request);
        let mut request = ordinary();
        request.headers.append("authorization", "private").unwrap();
        variants.push(request);
        for mut request in variants {
            assert!(prepare(&mut request).is_err());
            assert_eq!(request.destination, crate::fetch::RequestDestination::Fetch);
            assert_eq!(request.context, crate::fetch::RequestContext::Script);
        }
    }
}
