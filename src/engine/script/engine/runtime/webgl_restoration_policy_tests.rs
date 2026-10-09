//! Replay the actual restoration adapter against a recording native boundary.
//! Real native pixels/lifecycle are covered by the public-realm tests; this
//! isolates provider admission policy, which WARP-only unit tests cannot grant.
use super::webgl2_bindings_tests::{check, document};

fn replay(code: &str) {
    let (mut context, _host) = document();
    let source = format!(
        "(() => {{\n{}\n{}\n{}\n{}\n{}\n}})()",
        r#"
        const tasks = [], requests = [], events = [], errors = [];
        const queueWebGlContextTask = callback => tasks.push(callback);
        const markTrusted = event => event;
        const webGlPrivateBrands = () => new Map();
        const webGlToken = Symbol();
        const webGlExtensionFactories = new Map();
        const webGlCall = () => null;
        const flushWebGlCommands = () => {};
        const webGlError = (_, error) => errors.push(error);
        let active, admitted = 0;
        const context = {};
        const webGlState = receiver => {
            if (receiver !== context) throw TypeError('wrong context');
            return active;
        };
        const host = (operation, width, height, serialized) => {
            if (operation !== 'webglCreate') throw Error('unexpected native call');
            requests.push({width, height, options:JSON.parse(serialized)});
            return admitted;
        };
        const state = (attributes, api) => ({
            id:0, api, attributes, lost:true, epoch:3,
            lossEventCompleted:true, restoreAllowed:true, restoreQueued:false,
            simulatedLoss:true, objects:new Map(), extensions:new Map(),
            canvas:{width:17,height:19,dispatchEvent:event=>events.push(event.type)}
        });
        const assert = (value, message) => { if (!value) throw Error(message); };
        "#,
        include_str!("../../bootstrap/webgl_private_wire.js"),
        include_str!("../../bootstrap/webgl_attributes.js"),
        include_str!("../../bootstrap/webgl_lifecycle.js"),
        code,
    );
    check(&mut context, &source);
}

#[test]
fn restoration_forwards_every_converted_native_admission_option() {
    replay(
        r#"
        for (const api of ['webgl1','webgl2']) {
            for (const powerPreference of ['default','low-power','high-performance']) {
                for (const strict of [false,true]) {
                    let reads=0;
                    const dictionary={alpha:false,depth:false,stencil:true,
                        antialias:true,preserveDrawingBuffer:true,
                        get powerPreference(){reads++;return powerPreference},
                        failIfMajorPerformanceCaveat:strict};
                    const attributes=webGlContextAttributes(dictionary,api);
                    const initial=JSON.parse(webGlNativeOptions(attributes,api));
                    Object.defineProperty(dictionary,'powerPreference',
                        {get(){throw Error('restoration reread author options')}});
                    active=state(attributes,api);admitted=7;
                    restoreWebGlContext(context,true);
                    assert(requests.length===0,'native admission must be a task');
                    tasks.shift()();
                    const restored=requests.pop();
                    assert(reads===1,'dictionary must be converted only once');
                    assert(restored.width===17 && restored.height===19,'drawing-buffer extent');
                    assert(JSON.stringify(restored.options)===JSON.stringify(initial),
                        'initial and restored admission must match');
                    assert(restored.options.fail_if_major_performance_caveat===strict &&
                        restored.options.power_preference===powerPreference,'hardware policy lost');
                    assert(restored.options.antialias===(api==='webgl2'),'version-specific MSAA');
                    assert(!active.lost && active.id===7,'new native context must be installed');
                    assert(events.pop()==='webglcontextrestored','successful restoration event');
                    assert(!tasks.length && !errors.length,'no extra work or errors');
                }
            }
        }
        "#,
    );
}

#[test]
fn failed_strict_restoration_stays_lost_and_can_retry_without_relaxing_policy() {
    replay(
        r#"
        const attributes=webGlContextAttributes({failIfMajorPerformanceCaveat:true,
            powerPreference:'high-performance'},'webgl2');
        active=state(attributes,'webgl2');
        restoreWebGlContext(context,true);tasks.shift()();
        assert(active.lost && active.id===0 && !active.restoreQueued,'failed admission stays lost');
        assert(!events.length,'failed admission cannot dispatch restored');
        assert(requests[0].options.fail_if_major_performance_caveat,'strict policy missing');
        admitted=9;
        restoreWebGlContext(context,true);tasks.shift()();
        assert(requests.length===2,'explicit retry must attempt native creation');
        assert(JSON.stringify(requests[0])===JSON.stringify(requests[1]),'retry relaxed policy');
        assert(!active.lost && active.id===9 && events.length===1,'retry installs real context');
        "#,
    );
}
