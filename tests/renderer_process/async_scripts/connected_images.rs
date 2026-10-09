//! Real PNG decoding and browser response authority cross the contained renderer boundary.
use super::*;

fn png(color: [u8; 4]) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(2, 1, image::Rgba(color)))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[test]
fn connected_image_load_handlers_observe_native_pixels_and_final_origin_policy() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    for (response_type, expected) in [
        (FetchResponseType::Basic, "ready:30,60,90,255"),
        (FetchResponseType::Cors, "ready:30,60,90,255"),
        (
            FetchResponseType::Opaque,
            "blocked:SecurityError:255,0,0,255",
        ),
    ] {
        let mut driver = Driver::new(
            r#"<!doctype html><img id=photo src=/a.png><p id=status></p><script>
            photo.onload=async()=>{
                await photo.decode();photo.crossOrigin='anonymous';
                const output=document.getElementById('status');
                const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
                const ctx=canvas.getContext('2d');ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
                try {ctx.drawImage(photo,0,0);output.textContent='ready:'+ctx.getImageData(0,0,1,1).data.join();}
                catch(error){output.textContent='blocked:'+error.name+':'+ctx.getImageData(0,0,1,1).data.join();}
            };
            // Network completion is controlled by the test. Do not release it
            // on a parser-prefix paint before the load handler is installed.
            document.getElementById('status').textContent='pending';
        </script>"#,
        );
        driver.until_text("pending");
        driver.until_request("a.png");
        driver.respond_bytes_with_type(
            "a.png",
            &png([30, 60, 90, 255]),
            "image/png",
            200,
            response_type,
        );
        driver.until_text(expected);
        assert_eq!(
            driver.requests.len(),
            1,
            "Canvas must reuse the original image fetch"
        );
        driver.session.shutdown().unwrap();
    }
}

#[test]
fn connected_image_retargeting_waits_for_the_new_decoder_without_ghost_pixels() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><img id=photo src=/a.png><p id=result></p><script>
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');let loads=0;
        photo.onload=()=>{
            if(++loads===1){
                photo.src='/b.png';ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
                ctx.globalCompositeOperation='copy';ctx.drawImage(photo,0,0);
                result.textContent='waiting:'+ctx.getImageData(0,0,1,1).data.join();
            }else{ctx.drawImage(photo,0,0);result.textContent='replaced:'+ctx.getImageData(0,0,1,1).data.join();}
        };
        result.textContent='pending';
    </script>"#,
    );
    driver.until_text("pending");
    driver.until_request("a.png");
    driver.respond_bytes("a.png", &png([30, 60, 90, 255]), "image/png", 200);
    driver.until_text("waiting:255,0,0,255");
    driver.until_request("b.png");
    driver.respond_bytes("b.png", &png([0, 200, 10, 255]), "image/png", 200);
    driver.until_text("replaced:0,200,10,255");
    assert_eq!(driver.requests.len(), 2);
    driver.session.shutdown().unwrap();
}

#[test]
fn child_image_load_uses_child_owned_pixels_before_callback_dispatch() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><body><p id=result></p><script>
        setTimeout(()=>{const frame=document.createElement('iframe');frame.src='/child.html';document.body.append(frame);},1);
        result.textContent='pending';
        </script>"#,
    );
    driver.until_text("pending");
    driver.advance();
    driver.until_request("child.html");
    driver.respond_bytes("child.html", br#"<!doctype html><img id=photo src=/child.png><script>
        photo.onload=()=>{
            const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
            const ctx=canvas.getContext('2d');ctx.drawImage(photo,0,0);
            parent.document.getElementById('result').textContent='child:'+ctx.getImageData(0,0,1,1).data.join();
        };
        parent.document.getElementById('result').textContent='child handler ready';
    </script>"#, "text/html", 200);
    driver.until_text("child handler ready");
    driver.until_request("child.png");
    driver.respond_bytes("child.png", &png([30, 60, 90, 255]), "image/png", 200);
    driver.until_text("child:30,60,90,255");
    driver.session.shutdown().unwrap();
}
