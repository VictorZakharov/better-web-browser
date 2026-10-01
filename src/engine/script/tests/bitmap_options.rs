//! Observable Web IDL conversion, ordering and asynchronous error boundaries.
use super::*;

fn check(body: &str, expected: &str) {
    let source = format!("<body><output>pending</output><script>{body}</script></body>");
    let (dom, outcome) = execute_html(&source);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        expected
    );
}

#[test]
fn bitmap_dictionary_getters_are_lexicographic_and_read_exactly_once() {
    check(
        r#"
        const order = [], options = {};
        const members = ['colorSpaceConversion','imageOrientation','premultiplyAlpha',
            'resizeHeight','resizeQuality','resizeWidth'];
        for (const name of members) Object.defineProperty(options, name, {
            get() { order.push(name); return undefined; }
        });
        const pending = createImageBitmap(new ImageData(1,1), options);
        const synchronous = order.join(',') === members.join(',');
        pending.then(bitmap => {
            document.querySelector('output').textContent = [synchronous,order.length,bitmap.width].join(',');
        });
    "#,
        "true,6,1",
    );
}

#[test]
fn bitmap_dictionary_conversions_throw_before_returning_a_promise() {
    check(
        r#"
        const source = new ImageData(1,1), failures = [];
        for (const options of [1,true,'x',Symbol(), {imageOrientation:null},
            {resizeQuality:'cubic'}, {premultiplyAlpha:'other'},
            {colorSpaceConversion:'display-p3'}, {resizeWidth:1n}, {resizeHeight:Symbol()}]) {
            try { createImageBitmap(source,options); failures.push('returned'); }
            catch(error) { failures.push(error.name); }
        }
        document.querySelector('output').textContent = failures.every(value => value === 'TypeError');
    "#,
        "true",
    );
}

#[test]
fn throwing_option_getter_stops_later_getters_and_preserves_exception_identity() {
    check(
        r#"
        const marker = {}, order = [], options = {
            get colorSpaceConversion(){ order.push('color'); return 'default'; },
            get imageOrientation(){ order.push('orientation'); throw marker; },
            get resizeWidth(){ order.push('width'); return 1; }
        };
        let exact = false;
        try { createImageBitmap(new ImageData(1,1),options); }
        catch(error) { exact = error === marker; }
        document.querySelector('output').textContent = [exact,order.join('/')].join(',');
    "#,
        "true,color/orientation",
    );
}

#[test]
fn resize_enforce_range_rejects_infinity_nan_and_uint32_overflow() {
    check(
        r#"
        const failures = [];
        for (const number of [-1,Infinity,-Infinity,NaN,4294967296,Number.MAX_VALUE]) {
            for (const name of ['resizeWidth','resizeHeight']) {
                try { createImageBitmap(new ImageData(1,1),{[name]:number}); failures.push(false); }
                catch(error) { failures.push(error instanceof TypeError); }
            }
        }
        document.querySelector('output').textContent = failures.every(Boolean);
    "#,
        "true",
    );
}

#[test]
fn zero_resize_and_crop_are_rejected_promises_not_synchronous_exceptions() {
    check(
        r#"
        const source = new ImageData(1,1), tasks = [];
        tasks.push(createImageBitmap(source,{resizeWidth:0}));
        tasks.push(createImageBitmap(source,{resizeHeight:-0.9}));
        tasks.push(createImageBitmap(source,0,0,0,1));
        tasks.push(createImageBitmap(source,0,0,1,NaN));
        Promise.all(tasks.map(task => task.then(()=>'resolved',error=>error.name))).then(names=> {
            document.querySelector('output').textContent = names.join(',');
        });
    "#,
        "InvalidStateError,InvalidStateError,RangeError,RangeError",
    );
}

#[test]
fn cropped_signed_long_arguments_wrap_and_convert_left_to_right() {
    check(
        r#"
        const order = [], source = new ImageData(1,1);
        source.data.set([10,20,30,255]);
        const value = (name,number) => ({valueOf(){order.push(name);return number;}});
        const pending = createImageBitmap(source,value('x',4294967296),value('y',0),
            value('w',4294967297),value('h',1),{
                get resizeQuality(){order.push('quality');return 'pixelated';}
            });
        const synchronous = order.join(',');
        pending.then(bitmap=> {
            document.querySelector('output').textContent = [synchronous,bitmap.width,bitmap.height].join('|');
        });
    "#,
        "x,y,w,h,quality|1|1",
    );
}

#[test]
fn crop_bigint_and_symbol_arguments_throw_synchronously() {
    check(
        r#"
        const source = new ImageData(1,1), failures = [];
        for (const index of [0,1,2,3]) for (const value of [1n,Symbol()]) {
            const rectangle = [0,0,1,1]; rectangle[index] = value;
            try { createImageBitmap(source,...rectangle); failures.push(false); }
            catch(error){ failures.push(error.name === 'TypeError'); }
        }
        document.querySelector('output').textContent = failures.every(Boolean);
    "#,
        "true",
    );
}

#[test]
fn inferred_aspect_ratio_uses_ceiling_and_explicit_dimensions_use_truncation() {
    check(
        r#"
        const source = new ImageData(3,2);
        Promise.all([
            createImageBitmap(source,{resizeWidth:2}),
            createImageBitmap(source,{resizeHeight:1}),
            createImageBitmap(source,{resizeWidth:2.9,resizeHeight:3.9}),
            createImageBitmap(source,null),createImageBitmap(source,undefined)
        ]).then(bitmaps=> {
            document.querySelector('output').textContent = bitmaps.map(b=>b.width+'x'+b.height).join(',');
        });
    "#,
        "2x2,2x1,2x3,3x2,3x2",
    );
}

#[test]
fn required_arguments_and_invalid_overloads_throw_synchronously() {
    check(
        r#"
        const source = new ImageData(1,1), failures=[];
        for (const args of [[],[source,0,0],[source,0,0,1]]) {
            try { createImageBitmap(...args); failures.push(false); }
            catch(error) { failures.push(error instanceof TypeError); }
        }
        createImageBitmap(source,0,0,1,1,{},'ignored').then(bitmap=> {
            document.querySelector('output').textContent = [failures.every(Boolean),
                createImageBitmap.length,bitmap.width].join(',');
        });
    "#,
        "true,1,1",
    );
}

#[test]
fn invalid_sources_detached_data_and_closed_bitmaps_reject() {
    check(
        r#"
        const source = new ImageData(1,1);
        createImageBitmap(source).then(bitmap=> {
            bitmap.close();
            const data = new ImageData(1,1);
            structuredClone(data.data.buffer,{transfer:[data.data.buffer]});
            return Promise.all([{},null,undefined,bitmap,data].map(value=>
                createImageBitmap(value).then(()=>'resolved',error=>error.name)));
        }).then(names=>{document.querySelector('output').textContent=names.join(',');});
    "#,
        "TypeError,TypeError,TypeError,InvalidStateError,InvalidStateError",
    );
}

#[test]
fn imagebitmap_members_enforce_private_brand_checks() {
    check(
        r#"
        const failures = [], getter = Object.getOwnPropertyDescriptor(ImageBitmap.prototype,'width').get;
        for (const receiver of [{},null,undefined,Object.create(ImageBitmap.prototype)]) {
            try { getter.call(receiver); failures.push(false); } catch(error) { failures.push(error instanceof TypeError); }
            try { ImageBitmap.prototype.close.call(receiver); failures.push(false); } catch(error) { failures.push(error instanceof TypeError); }
        }
        try { new ImageBitmap(); failures.push(false); } catch(error) { failures.push(error instanceof TypeError); }
        document.querySelector('output').textContent = failures.every(Boolean);
    "#,
        "true",
    );
}

#[test]
fn bitmap_budget_rejections_do_not_allocate_wrapped_or_oversized_rasters() {
    check(
        r#"
        const source = new ImageData(1,1);
        Promise.all([
            createImageBitmap(source,{resizeWidth:4294967295,resizeHeight:4294967295}),
            createImageBitmap(source,0,0,-2147483648,1)
        ].map(p=>p.then(()=>'resolved',error=>error.name))).then(names=> {
            document.querySelector('output').textContent=names.join(',');
        });
    "#,
        "NotSupportedError,NotSupportedError",
    );
}
