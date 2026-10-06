use super::*;

const AHEM: &[u8] = include_bytes!("../../canvas/fonts/ahem.ttf");

fn styled_driver(sources: &str) -> Driver {
    Driver::new(&format!(
        r#"<!doctype html><style id=fonts>
        @font-face{{font-family:Remote;src:{sources};unicode-range:U+41-5A}}
        #sample{{font:20px Remote}}</style><p id=sample>AAAA</p><p id=status>pending</p>
        <script>
        const face=[...document.fonts][0];
        const timer=setInterval(()=>{{
            if (!document.fonts.check('20px Remote','AAAA')) return;
            const canvas=document.createElement('canvas'),ctx=canvas.getContext('2d');
            ctx.font='20px Remote';
            status.textContent=face.status+':'+ctx.measureText('AAAA').width;
            clearInterval(timer);
        }},1);
        </script>"#
    ))
}

#[test]
fn stylesheet_font_http_and_decode_failures_advance_one_source_at_a_time_to_real_font_bytes() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = styled_driver("url(/missing.ttf),url(/corrupt.ttf),url(/ahem.ttf)");
    driver.until_request("missing.ttf");
    assert!(
        !driver
            .requests
            .keys()
            .any(|url| url.ends_with("corrupt.ttf") || url.ends_with("ahem.ttf"))
    );
    driver.respond_bytes("missing.ttf", b"not found", "font/ttf", 404);
    driver.until_request("corrupt.ttf");
    assert!(!driver.requests.keys().any(|url| url.ends_with("ahem.ttf")));
    driver.respond_bytes("corrupt.ttf", b"not an SFNT", "font/ttf", 200);
    driver.until_request("ahem.ttf");
    driver.respond_bytes("ahem.ttf", AHEM, "font/ttf", 200);
    driver.until_text("loaded:80");
    assert_eq!(driver.requests.len(), 3);
    driver.session.shutdown().unwrap();
}

#[test]
fn stylesheet_font_opaque_response_is_never_installed_and_advances_to_clean_candidate() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = styled_driver("url(/opaque.ttf),url(/clean.ttf)");
    driver.until_request("opaque.ttf");
    driver.respond_bytes_with_type(
        "opaque.ttf",
        AHEM,
        "font/ttf",
        200,
        FetchResponseType::Opaque,
    );
    driver.until_request("clean.ttf");
    driver.respond_bytes("clean.ttf", AHEM, "font/ttf", 200);
    driver.until_text("loaded:80");
    assert_eq!(driver.requests.len(), 2);
    driver.session.shutdown().unwrap();
}

#[test]
fn unsupported_font_source_hints_are_not_downloaded_and_success_stops_before_later_candidates() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = styled_driver(
        "url(/unsupported.ttf) format(unknown),local('Unavailable'),url(/good.ttf),url(/unused.ttf)",
    );
    driver.until_request("good.ttf");
    driver.respond_bytes("good.ttf", AHEM, "font/ttf", 200);
    driver.until_text("loaded:80");
    assert_eq!(driver.requests.len(), 1);
    driver.session.shutdown().unwrap();
}

#[test]
fn explicit_loading_of_unused_css_face_retains_the_whole_ordered_source_list() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
        @font-face{font-family:Unused;src:url(/first.ttf),url(/second.ttf)}
        </style><p id=status>pending</p><script>
        document.fonts.load('20px Unused','A').then(faces=>{
            const ctx=document.createElement('canvas').getContext('2d');
            ctx.font='20px Unused';
            status.textContent=faces.length+':'+faces[0].status+':'+ctx.measureText('AAAA').width;
        },error=>status.textContent=error.name);
        </script>"#,
    );
    driver.until_request("first.ttf");
    driver.respond_bytes("first.ttf", b"missing", "font/ttf", 404);
    driver.until_request("second.ttf");
    driver.respond_bytes("second.ttf", AHEM, "font/ttf", 200);
    driver.until_text("1:loaded:80");
    assert_eq!(driver.requests.len(), 2);
    driver.session.shutdown().unwrap();
}
