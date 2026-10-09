//! Untrusted direct calls cannot partially admit a packet or retain racing bytes.
use super::webgl2_bindings_tests::check;
use super::*;
use crate::engine::script::{HostState, module_loader::WebModuleLoader};

fn raw() -> (Context, Rc<RefCell<HostState>>) {
    let host = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
    let id = host.borrow_mut().webgl.create(2, 2, "{}").unwrap();
    let operations = include_str!("../../../webgl/numeric_packet/operations.json");
    check(
        &mut context,
        &format!(
            r#"
        const id={id},operations={operations};
        const code=name=>operations.indexOf(name);
        const record=(name,i=[],f=[])=>[code(name),id,i.length,f.length,...i,...f];
        const native=command=>JSON.parse(__hostCall('webglCommand',id,JSON.stringify(command)));
        const send=values=>__hostCall('webglCommandPacket',new Float64Array(values),values.length);
        const reject=(view,count)=>{{let threw=false;
            try{{__hostCall('webglCommandPacket',view,count)}}catch(e){{threw=e instanceof TypeError}}
            if(!threw)throw Error('malformed packet admitted');}};
        native({{op:'clearColor',f:[1,0,0,1]}});native({{op:'clear',i:[16384]}});
    "#
        ),
    );
    (context, host)
}

#[test]
fn webgl_packet_rejection_is_atomic_and_never_invokes_author_coercion() {
    let (mut context, _host) = raw();
    check(
        &mut context,
        r#"
        const valid=record('clearColor',[],[0,1,0,1]);
        for(const suffix of [[0],[0,id],[0,id,0],[999999,id,0,0],[0,0,0,0],[0,id,65,0]]) {
            const values=[...valid,...suffix];reject(new Float64Array(values),values.length);
            if(String(native({op:'getParameter',i:[3106]}))!=='1,0,0,1')throw Error('partial admission');
        }
        let hooks=0;const object={valueOf(){hooks++;return valid.length}};
        for(const count of [object,1n,'8',null,undefined,NaN,Infinity,-1,.5,8705])
            reject(new Float64Array(valid),count);
        for(const view of [valid,new Proxy(new Float64Array(valid),{}),new Float32Array(valid),
            new DataView(new ArrayBuffer(64)),null,new Float64Array(8705)])reject(view,valid.length);
        if(hooks || native({op:'getError'})!==0)throw Error('coercion or GL error during rejection');
    "#,
    );
}

#[test]
fn webgl_packet_copy_uses_the_actual_fixed_view_and_ignores_author_accessors() {
    let (mut context, _host) = raw();
    check(
        &mut context,
        r#"
        const values=[...record('clearColor',[],[0,1,0,1]),...record('clear',[16384])];
        const backing=new Float64Array(values.length+6);backing.fill(NaN);backing.set(values,3);
        const view=new Float64Array(backing.buffer,24,values.length),fail=()=>{throw Error('author accessor')};
        for(const name of ['buffer','byteOffset','byteLength','length'])Object.defineProperty(view,name,{get:fail});
        const lost=__hostCall('webglCommandPacket',view,values.length);
        backing.fill(17);backing.buffer.transfer();
        if(lost.length || String(native({op:'getParameter',i:[3106]}))!=='0,1,0,1')throw Error('owned prefix/offset');
        if(native({op:'getError'})!==0)throw Error('valid owned packet errors');
        const pixels=__hostCall('webglReadPixels',id,JSON.stringify({op:'readPixels',i:[0,0,1,1,6408,5121,4]}));
        if(String(pixels)!=='0,255,0,255')throw Error('owned native pixel');
    "#,
    );
}

#[test]
fn webgl_packet_rejects_shared_resizable_and_detached_storage() {
    let (mut context, _host) = raw();
    check(
        &mut context,
        r#"
        const values=record('clearColor',[],[0,1,0,1]);
        const shared=new Float64Array(new SharedArrayBuffer(64));shared.set(values);reject(shared,8);
        const resizable=new Float64Array(new ArrayBuffer(64,{maxByteLength:128}));
        resizable.set(values);reject(resizable,8);
        const detached=new Float64Array(values);detached.buffer.transfer();reject(detached,8);
        const small=new Float64Array(4);reject(small,8);
        if(String(native({op:'getParameter',i:[3106]}))!=='1,0,0,1'||native({op:'getError'})!==0)
            throw Error('rejected buffer changed native state');
    "#,
    );
}

#[test]
fn webgl_packet_reply_has_own_elements_and_does_not_route_to_a_peer() {
    let (mut context, _host) = raw();
    let mut peer = crate::engine::webgl::Contexts::default();
    let peer_id = peer.create(2, 2, "{}").unwrap();
    check(
        &mut context,
        &format!(
            r#"
        const values=record('clear',[16384]);values[1]={peer_id};
        const old=Object.getOwnPropertyDescriptor(Array.prototype,'0');let hooks=0;
        Object.defineProperty(Array.prototype,'0',{{configurable:true,get(){{hooks++;throw Error('reply getter')}},
            set(){{hooks++;throw Error('reply setter')}}}});
        let reply;
        try{{reply=__hostCall('webglCommandPacket',new Float64Array(values),values.length)}}
        finally{{if(old)Object.defineProperty(Array.prototype,'0',old);else delete Array.prototype[0]}}
        if(hooks||reply.length!==1||!Object.hasOwn(reply,'0')||reply[0]!=={peer_id})throw Error('loss reply');
    "#
        ),
    );
    assert!(peer.snapshot(peer_id).is_some());
}
