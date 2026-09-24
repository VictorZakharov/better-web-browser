use super::*;

#[test]
fn script_created_font_registers_before_its_loaded_promise_updates_the_page() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><body><p id=status>pending</p><script>\
         const bytes=new Uint8Array(76);\
         bytes.set([119,79,70,70,0,1,0,0,0,0,0,76,0,1,0,0]);\
         bytes.set([0,0,0,40],16);\
         bytes.set([104,101,97,100,0,0,0,64,0,0,0,12,0,0,0,12],44);\
         const face=new FontFace('Test Fixture',bytes);\
         document.fonts.add(face);\
         face.loaded.then(()=>{\
           document.getElementById('status').textContent=\
             document.fonts.check('12px Test Fixture')?'font loaded':'font missing';\
         });\
         </script>",
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
    driver.respond_bytes("remote.woff", &synthetic_woff(), "font/woff", 200);
    driver.until_text("remote loaded");
    driver.session.shutdown().unwrap();
}

fn synthetic_woff() -> Vec<u8> {
    let mut bytes = vec![0; 76];
    bytes[..4].copy_from_slice(b"wOFF");
    bytes[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    bytes[8..12].copy_from_slice(&76_u32.to_be_bytes());
    bytes[12..14].copy_from_slice(&1_u16.to_be_bytes());
    bytes[16..20].copy_from_slice(&40_u32.to_be_bytes());
    bytes[44..48].copy_from_slice(b"head");
    bytes[48..52].copy_from_slice(&64_u32.to_be_bytes());
    bytes[52..56].copy_from_slice(&12_u32.to_be_bytes());
    bytes[56..60].copy_from_slice(&12_u32.to_be_bytes());
    bytes
}
