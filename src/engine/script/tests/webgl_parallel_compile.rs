use super::*;

#[test]
fn parallel_compile_extension_admits_real_queries_without_native_thread_controls() {
    let (_, outcome) = execute_html(
        r#"<script>
        const assert=(value,label)=>{if(!value)throw Error(label)};
        for(const api of ['webgl','webgl2']) {
            const canvas=document.createElement('canvas'),gl=canvas.getContext(api);
            const program=gl.createProgram(),shader=gl.createShader(gl.VERTEX_SHADER);
            assert(gl.getProgramParameter(program,0x91b1)===null,'enum requires admission');
            assert(gl.getError()===gl.INVALID_ENUM,'enum error before admission');
            const available=gl.getSupportedExtensions().includes('KHR_parallel_shader_compile');
            const ext=gl.getExtension('KHR_parallel_shader_compile');
            assert(Boolean(ext)===available,'availability and admission agree');
            if(!ext)continue;
            assert(ext===gl.getExtension('khr_PARALLEL_shader_COMPILE'),'context cached object');
            assert(Object.prototype.toString.call(ext)==='[object KHR_parallel_shader_compile]','extension brand');
            assert(ext.COMPLETION_STATUS_KHR===0x91b1,'completion constant');
            assert(!('KHR_parallel_shader_compile' in globalThis)&&
                !('MAX_SHADER_COMPILER_THREADS_KHR' in ext)&&
                !('maxShaderCompilerThreadsKHR' in ext),'WebGL does not expose native thread controls');
            const constant=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(ext),'COMPLETION_STATUS_KHR');
            assert(constant.enumerable&&!constant.writable&&!constant.configurable,'IDL constant descriptor');
            assert(gl.getProgramParameter(program,ext.COMPLETION_STATUS_KHR)===true,'unlinked completion');
            assert(gl.getShaderParameter(shader,ext.COMPLETION_STATUS_KHR)===true,'uncompiled completion');
            const source=api==='webgl2'?'#version 300 es\nvoid main(){gl_Position=vec4(0.0);}':
                'void main(){gl_Position=vec4(0.0);}';
            gl.shaderSource(shader,source);gl.compileShader(shader);
            assert(typeof gl.getShaderParameter(shader,ext.COMPLETION_STATUS_KHR)==='boolean','native completion value');
            assert(gl.getShaderParameter(shader,gl.COMPILE_STATUS)===true,'real compiler success');
            assert(gl.getShaderParameter(shader,ext.COMPLETION_STATUS_KHR)===true,'finished compiler completion');
            gl.getParameter(0x91b0);assert(gl.getError()===gl.INVALID_ENUM,'native max threads enum excluded');
            gl.getParameter(ext.COMPLETION_STATUS_KHR);assert(gl.getError()===gl.INVALID_ENUM,'completion not general parameter');
            const foreign=document.createElement('canvas').getContext(api).createProgram();
            assert(gl.getProgramParameter(foreign,ext.COMPLETION_STATUS_KHR)===null,'foreign program rejected');
            assert(gl.getError()===gl.INVALID_OPERATION,'foreign program ownership');
            assert(gl.getError()===gl.NO_ERROR,'queries left clean driver state');
        }
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn completion_polling_terminates_after_loss_and_requires_readmission_after_restore() {
    super::webgl_lifecycle::run(
        r#"
        const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl2');
        const ext=gl.getExtension('KHR_parallel_shader_compile');
        if(!ext){canvas.dataset.result='pass';}else{
            const assert=(value,label)=>{if(!value)throw Error(label)};
            const program=gl.createProgram(),shader=gl.createShader(gl.VERTEX_SHADER);
            const loss=gl.getExtension('WEBGL_lose_context');
            canvas.addEventListener('webglcontextlost',event=>{
                event.preventDefault();
                assert(gl.getProgramParameter(program,ext.COMPLETION_STATUS_KHR)===true,'lost program completion');
                assert(gl.getShaderParameter(shader,ext.COMPLETION_STATUS_KHR)===true,'lost shader completion');
                assert(gl.getProgramParameter(program,gl.LINK_STATUS)===null,'ordinary lost query remains null');
                // WEBGL_lose_context disallows restoration until dispatch of
                // the canceled context-lost event has completed.
                setTimeout(()=>loss.restoreContext(),0);
            });
            canvas.addEventListener('webglcontextrestored',()=>{
                const fresh=gl.createProgram();
                assert(gl.getProgramParameter(fresh,ext.COMPLETION_STATUS_KHR)===null,'restoration clears admission');
                assert(gl.getError()===gl.INVALID_ENUM,'restored query enum error');
                const again=gl.getExtension('KHR_parallel_shader_compile');
                assert(again&&again!==ext,'restoration creates new extension object');
                assert(gl.getProgramParameter(fresh,again.COMPLETION_STATUS_KHR)===true,'fresh native completion');
                assert(gl.getError()===gl.NO_ERROR,'clean restored driver');
                canvas.dataset.result='pass';
            });
            loss.loseContext();
            assert(gl.getProgramParameter(program,ext.COMPLETION_STATUS_KHR)===true,'immediate loss completion');
        }
        "#,
    );
}
