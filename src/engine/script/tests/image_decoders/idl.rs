use super::check;

#[test]
fn decoder_dictionary_getters_convert_once_in_alphabetical_order() {
    check(
        r#"
        const visits=[],values={colorSpaceConversion:'none',data:pngBytes(),desiredHeight:1,
            desiredWidth:2,preferAnimation:false,transfer:[],type:'image/png'};
        const init={};
        for(const key of Object.keys(values))Object.defineProperty(init,key,{get(){visits.push(key);return values[key];}});
        const decoder=new ImageDecoder(init);
        assert(visits.join(',')===Object.keys(values).join(','),'WebIDL conversion order and single access');
        (await decoder.decode()).image.close();decoder.close();
    "#,
    );
}

#[test]
fn decode_dictionary_converts_boolean_before_frame_index_once() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes()}),visits=[];
        const output=await decoder.decode({get frameIndex(){visits.push('index');return 0.9;},
            get completeFramesOnly(){visits.push('complete');return false;}});
        assert(visits.join(',')==='complete,index','ordered decode options');output.image.close();
        for(const frameIndex of [-1,Infinity,NaN,2**32,1n,Symbol('index')]) {
            let name;try{await decoder.decode({frameIndex});}catch(e){name=e.name;}
            assert(name==='TypeError','frame index uses EnforceRange unsigned long');
        }
        decoder.close();
    "#,
    );
}

#[test]
fn promise_operations_reject_symbol_and_missing_arguments_without_throwing() {
    check(
        r#"
        for(const call of [()=>ImageDecoder.isTypeSupported(),()=>ImageDecoder.isTypeSupported(Symbol('mime'))]) {
            let promise,sync=false;try{promise=call();}catch(e){sync=true;}
            assert(!sync&&promise instanceof Promise,'promise conversion must reject');
            let name;try{await promise;}catch(e){name=e.name;}assert(name==='TypeError','conversion rejection');
        }
        for(const type of ['','not-a-mime','text/plain','image/']) {
            let name;try{await ImageDecoder.isTypeSupported(type);}catch(e){name=e.name;}
            assert(name==='TypeError','invalid image MIME rejects');
        }
        assert(!(await ImageDecoder.isTypeSupported('image/unknown')),'valid unsupported MIME resolves false');
    "#,
    );
}

#[test]
fn tracks_have_readonly_indexed_properties_and_stable_identity() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/gif',data:gifBytes()});await decoder.tracks.ready;
        const list=decoder.tracks,track=list[0];
        assert(list instanceof ImageTrackList&&track instanceof ImageTrack,'native interface identities');
        assert(Object.prototype.toString.call(list)==='[object ImageTrackList]','list brand');
        assert('0' in list&&!('1' in list)&&list[1]===undefined,'supported indices');
        assert(Object.keys(list).join(',')==='0','indexed property enumeration');
        const descriptor=Object.getOwnPropertyDescriptor(list,'0');
        assert(descriptor.value===track&&!descriptor.writable&&descriptor.enumerable,'readonly indexed descriptor');
        assert(!Reflect.set(list,'0',null)&&!Reflect.deleteProperty(list,'0'),'cannot overwrite or delete index');
        assert(!Reflect.defineProperty(list,'0',{value:null}),'cannot redefine index');
        list.note='author';assert(list.note==='author','ordinary expando remains possible');
        decoder.close();
        assert(list.length===0&&!('0' in list)&&list[0]===undefined,'closed list drops indexed properties');
        assert(list.note==='author'&&!track.selected,'ordinary properties and retained track lifetime');
    "#,
    );
}

#[test]
fn interfaces_use_webidl_enumerability_and_illegal_constructor_checks() {
    check(
        r#"
        const supportDescriptor=Object.getOwnPropertyDescriptor(ImageDecoder,'isTypeSupported');
        assert(supportDescriptor.enumerable&&supportDescriptor.configurable&&supportDescriptor.writable,'static WebIDL operation descriptor');
        for(const Constructor of [ImageTrack,ImageTrackList]) {
            let name;try{new Constructor();}catch(e){name=e.name;}assert(name==='TypeError','illegal constructor');
        }
        for(const [Constructor,key] of [[ImageDecoder,'tracks'],[ImageTrack,'selected'],[ImageTrackList,'ready']]) {
            const descriptor=Object.getOwnPropertyDescriptor(Constructor.prototype,key);
            assert(descriptor.enumerable&&descriptor.configurable,'WebIDL attribute descriptor');
            let name;try{descriptor.get.call({});}catch(e){name=e.name;}assert(name==='TypeError','branded getter');
        }
        for(const method of ['reset','close']) {
            let name;try{ImageDecoder.prototype[method].call({});}catch(e){name=e.name;}
            assert(name==='TypeError','branded '+method);
        }
        let name;try{await ImageDecoder.prototype.decode.call({});}catch(e){name=e.name;}
        assert(name==='TypeError','promise-returning branded decode');
    "#,
    );
}

#[test]
fn duplicate_transfer_is_checked_after_all_dictionary_members_are_converted() {
    check(
        r#"
        const bytes=pngBytes(),visits=[];
        let name;try{new ImageDecoder({data:bytes,transfer:[bytes.buffer,bytes.buffer],
            get type(){visits.push('type');return 'image/png';}});}catch(e){name=e.name;}
        assert(name==='DataCloneError'&&visits.join(',')==='type'&&bytes.byteLength>0,'conversion before transfer validation');
    "#,
    );
}

#[test]
fn mime_validation_rejects_malformed_parameters_and_accepts_quoted_values() {
    check(
        r#"
        for(const type of ['image/png;broken','image/png;name=','image/png;name="unterminated',
            'image/png\r\nX-Header: bad','image/png;name="line\nfeed"']) {
            let name;try{await ImageDecoder.isTypeSupported(type);}catch(e){name=e.name;}
            assert(name==='TypeError','malformed MIME parameter: '+type);
        }
        for(const type of ['image/png;name=foo','image/png; name="a;b"','IMAGE/PNG;name="escaped\\\"quote"'])
            assert(await ImageDecoder.isTypeSupported(type),'valid quoted MIME parameter');
    "#,
    );
}
