use super::*;
use base64::Engine;

const AHEM: &[u8] = include_bytes!("../../canvas/fonts/ahem.ttf");

#[test]
fn script_created_font_registers_before_its_loaded_promise_updates_the_page() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        &"<!doctype html><body><p id=status>pending</p><script>\
         const bytes=Uint8Array.from(atob('@AHEM@'),c=>c.charCodeAt(0));\
         const face=new FontFace('Test Fixture',bytes);\
         document.fonts.add(face);\
         face.loaded.then(()=>{\
           document.getElementById('status').textContent=\
             document.fonts.check('12px Test Fixture')?'font loaded':'font missing';\
         });\
         </script>"
            .replace(
                "@AHEM@",
                &base64::engine::general_purpose::STANDARD.encode(AHEM),
            ),
    );
    driver.until_text("font loaded");
    driver.session.shutdown().unwrap();
}

#[test]
fn url_backed_font_installs_at_network_completion_without_an_extra_clock_tick() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><body><p id=status>pending</p><script>\
         const face=new FontFace('Remote Fixture','url(/remote.woff)');\
         document.fonts.add(face);\
         face.load().then(()=>document.getElementById('status').textContent='remote loaded',\
           ()=>document.getElementById('status').textContent='remote failed');\
         </script>",
    );
    driver.until_text("pending");
    driver.advance();
    driver.until_request("remote.woff");
    driver.respond_bytes("remote.woff", AHEM, "font/ttf", 200);
    driver.until_text("remote loaded");
    driver.session.shutdown().unwrap();
}
