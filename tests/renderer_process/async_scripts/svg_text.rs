//! SVG text must publish real, updated pixels across the renderer boundary.
use super::*;

const AHEM: &[u8] = include_bytes!("../../canvas/fonts/ahem.ttf");

fn svg_image(presentation: &RendererPresentation) -> &better_web_browser::engine::DecodedImage {
    &presentation
        .images
        .iter()
        .find(|image| image.url.starts_with("inline-svg:"))
        .unwrap_or_else(|| {
            panic!(
                "SVG raster update missing; image keys: {:?}; text: {}",
                presentation
                    .images
                    .iter()
                    .map(|image| &image.url)
                    .collect::<Vec<_>>(),
                painted_text(presentation)
            )
        })
        .image
}

fn assert_blue_rectangle(image: &better_web_browser::engine::DecodedImage, rectangle: [u32; 4]) {
    let [left, top, right, bottom] = rectangle;
    for y in 0..image.height {
        for x in 0..image.width {
            let index = (y as usize * image.width as usize + x as usize) * 4;
            let expected = if x >= left && x < right && y >= top && y < bottom {
                [255, 0, 0, 255]
            } else {
                [0; 4]
            };
            assert_eq!(&image.bgra[index..index + 4], expected, "({x},{y})");
        }
    }
}

#[test]
fn font_ready_waits_for_svg_outline_publication_not_just_canvas_metrics() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
      @font-face{font-family:SvgFixture;src:url(/svg-ahem.ttf)}
      svg{font-family:SvgFixture;font-size:20px}
      </style><svg width=100 height=50><text x=10 y=30 fill=blue>AAAA</text></svg>
      <p id=status>pending</p><script>
      document.fonts.ready.then(()=>{
        const context=document.createElement('canvas').getContext('2d');context.font='20px SvgFixture';
        status.textContent='ready:'+document.fonts.status+':'+context.measureText('AAAA').width;
      });</script>"#,
    );
    let pending = driver.until_text("pending");
    let previous = svg_image(&pending).bgra.clone();
    driver.until_request("svg-ahem.ttf");
    driver.respond_bytes("svg-ahem.ttf", AHEM, "font/ttf", 200);
    let ready = driver.until_text("ready:loaded:80");
    let image = svg_image(&ready);
    assert_ne!(
        image.bgra, previous,
        "font loading must invalidate the fallback SVG pixels"
    );
    assert_blue_rectangle(image, [10, 14, 90, 34]);
    assert_eq!(driver.requests.len(), 1, "one real font download");
    driver.session.shutdown().unwrap();
}

#[test]
fn svg_presentation_font_size_mutation_rerenders_loaded_outlines_without_refetch() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
      @font-face{font-family:SvgFixture;src:url(/svg-ahem.ttf)}
      svg{font-family:SvgFixture}
      </style><svg width=120 height=80><text id=label x=10 y=40 font-size=20 fill=blue>AA</text></svg>
      <p id=status>pending</p><script>
      document.fonts.ready.then(()=>{
        status.textContent='small';
        setTimeout(()=>{document.getElementById('label').setAttribute('font-size','40');status.textContent='large';},1);
      });</script>"#,
    );
    driver.until_request("svg-ahem.ttf");
    driver.respond_bytes("svg-ahem.ttf", AHEM, "font/ttf", 200);
    let small = driver.until_text("small");
    assert_blue_rectangle(svg_image(&small), [10, 24, 50, 44]);
    driver.advance();
    let large = driver.until_text("large");
    assert_blue_rectangle(svg_image(&large), [10, 8, 90, 48]);
    assert_eq!(driver.requests.len(), 1);
    driver.session.shutdown().unwrap();
}

#[test]
fn svg_removal_retires_the_published_key_and_reattachment_resends_pixels() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html>
      <svg id=art width=10 height=10><rect width=10 height=10 fill=blue /></svg>
      <p id=status>before</p><script>
      const art = document.getElementById('art');
      setTimeout(()=>{
        art.remove();status.textContent='removed';
        setTimeout(()=>{document.body.appendChild(art);status.textContent='restored';},1);
      },1);</script>"#,
    );
    let before = driver.until_text("before");
    let image = svg_image(&before).bgra.clone();
    let key = before
        .images
        .iter()
        .find(|image| image.url.starts_with("inline-svg:"))
        .unwrap()
        .url
        .clone();
    driver.advance();
    let removed = driver.until_text("removed");
    assert!(removed.retired_image_keys.contains(&key));
    assert!(!removed.images.iter().any(|image| image.url == key));
    driver.advance();
    let restored = driver.until_text("restored");
    assert_eq!(svg_image(&restored).bgra, image);
    assert!(restored.images.iter().any(|image| image.url == key));
    driver.session.shutdown().unwrap();
}
