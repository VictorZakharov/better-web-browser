//! Query shape and resource lifetime are derived from linked driver metadata.
use super::webgl_instancing::check;

const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const shader=(kind,source)=>{const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));return s;};
    const link=source=>{const p=gl.createProgram();
        gl.attachShader(p,shader(gl.VERTEX_SHADER,'void main(){gl_Position=vec4(0.);gl_PointSize=1.;}'));
        gl.attachShader(p,shader(gl.FRAGMENT_SHADER,'precision mediump float;'+source));
        gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));return p;};
    const location=(p,name)=>{const l=gl.getUniformLocation(p,name);assert(l,'missing '+name);return l;};
"#;

#[test]
fn webgl_uniform_scalar_vector_boolean_and_sampler_shapes_match_native_types() {
    check(&format!(
        "{SETUP}{}",
        r#"
        const p=link('uniform float scalar;uniform int integer;uniform bool flag;uniform bvec3 flags;'+
            'uniform ivec4 indices;uniform vec2 pair;uniform sampler2D image;'+
            'void main(){gl_FragColor=vec4(scalar+float(integer)+float(flag)+float(flags.x)+float(flags.y)+float(flags.z)'+
            '+float(indices.x+indices.y+indices.z+indices.w)+pair.x+pair.y)+texture2D(image,vec2(0.));}');
        gl.useProgram(p);
        const scalar=location(p,'scalar'),integer=location(p,'integer'),flag=location(p,'flag');
        const flags=location(p,'flags'),indices=location(p,'indices'),pair=location(p,'pair'),image=location(p,'image');
        gl.uniform1f(scalar,0.25);gl.uniform1i(integer,-17);gl.uniform1i(flag,1);
        gl.uniform3iv(flags,new Int32Array([0,3,-1]));gl.uniform4iv(indices,new Int32Array([-2,3,4,5]));
        gl.uniform2fv(pair,new Float32Array([0.5,0.75]));gl.uniform1i(image,1);
        assert(gl.getUniform(p,scalar)===0.25 && gl.getUniform(p,integer)===-17,'scalar output');
        assert(gl.getUniform(p,flag)===true,'boolean scalar');
        const bools=gl.getUniform(p,flags),ints=gl.getUniform(p,indices),floats=gl.getUniform(p,pair);
        assert(Array.isArray(bools) && bools.every(v=>typeof v==='boolean') && String(bools)==='false,true,true','boolean vector');
        assert(ints instanceof Int32Array && String(ints)==='-2,3,4,5','integer vector');
        assert(floats instanceof Float32Array && String(floats)==='0.5,0.75','float vector');
        assert(gl.getUniform(p,image)===1,'sampler scalar');
        ints[0]=99;floats[0]=99;bools[0]=true;
        assert(gl.getUniform(p,indices)[0]===-2 && gl.getUniform(p,pair)[0]===0.5 && !gl.getUniform(p,flags)[0],'fresh query snapshots');
        assert(gl.getError()===0,'typed uniform queries');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_uniform_matrix_queries_preserve_column_major_component_counts() {
    check(&format!(
        "{SETUP}{}",
        r#"
        const p=link('uniform mat2 a;uniform mat3 b;uniform mat4 c;void main(){'+
            'gl_FragColor=vec4(a[0][0]+a[1][1]+b[0][0]+b[1][1]+b[2][2]+c[0][0]+c[1][1]+c[2][2]+c[3][3]);}');
        gl.useProgram(p);
        for(const [name,size] of [['a',2],['b',3],['c',4]]) {
            const l=location(p,name), values=Float32Array.from({length:size*size},(_,i)=>i/4);
            gl['uniformMatrix'+size+'fv'](l,false,values);
            const actual=gl.getUniform(p,l);
            assert(actual instanceof Float32Array && actual.length===size*size && String(actual)===String(values),'matrix '+name);
            gl['uniformMatrix'+size+'fv'](l,true,values);
            assert(gl.getError()===gl.INVALID_VALUE,'transpose is forbidden in WebGL 1');
            assert(String(gl.getUniform(p,l))===String(values),'failed upload preserves matrix');
        }
        assert(gl.getError()===0,'matrix queries');document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_uniform_array_and_struct_locations_resolve_individual_elements() {
    check(&format!(
        "{SETUP}{}",
        r#"
        const p=link('struct Light{vec3 color;float strength;};uniform Light lights[2];uniform float weights[3];'+
            'void main(){gl_FragColor=vec4(lights[0].color*lights[0].strength+lights[1].color*lights[1].strength,'+
            'weights[0]+weights[1]+weights[2]);}');
        gl.useProgram(p);
        const weights=location(p,'weights'),w0=location(p,'weights[0]'),w1=location(p,'weights[1]'),w2=location(p,'weights[2]');
        gl.uniform1fv(weights,new Float32Array([0.25,0.5,0.75]));
        assert(gl.getUniform(p,w0)===0.25 && gl.getUniform(p,w1)===0.5 && gl.getUniform(p,w2)===0.75,'array queries read one element');
        for(let i=0;i<2;i++) {
            const color=location(p,'lights['+i+'].color'),strength=location(p,'lights['+i+'].strength');
            gl.uniform3f(color,i,0.5,1);gl.uniform1f(strength,i+0.25);
            assert(gl.getUniform(p,color) instanceof Float32Array && String(gl.getUniform(p,color))===i+',0.5,1','struct vector '+i);
            assert(gl.getUniform(p,strength)===i+0.25,'struct scalar '+i);
        }
        assert(gl.getUniformLocation(p,'weights[3]')===null && gl.getUniformLocation(p,'lights[2].color')===null,'out-of-range location');
        assert(gl.getUniformLocation(p,'lights.color')===null && gl.getUniformLocation(p,'absent')===null,'inactive/invalid name');
        assert(gl.getError()===0,'array family resolution');document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_uniform_locations_are_owned_by_program_and_link_generation() {
    check(&format!(
        "{SETUP}{}",
        r#"
        const source='uniform float value;void main(){gl_FragColor=vec4(value);}';
        const p=link(source),other=link(source),old=location(p,'value');
        gl.useProgram(p);gl.uniform1f(old,0.25);
        assert(gl.getUniform(other,old)===null && gl.getError()===gl.INVALID_OPERATION,'foreign program query');
        gl.useProgram(other);gl.uniform1f(old,0.75);
        assert(gl.getError()===gl.INVALID_OPERATION && gl.getUniform(p,old)===0.25,'foreign program upload');
        gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),'relink');
        assert(gl.getUniform(p,old)===null && gl.getError()===gl.INVALID_OPERATION,'stale query');
        gl.useProgram(p);gl.uniform1f(old,1);assert(gl.getError()===gl.INVALID_OPERATION,'stale upload');
        const fresh=location(p,'value');assert(fresh!==old && gl.getUniform(p,fresh)===0,'fresh link initializes uniforms');
        gl.uniform1f(fresh,0.5);assert(gl.getUniform(p,fresh)===0.5,'fresh location works');
        gl.uniform1f(null,1);assert(gl.getError()===0,'nullable upload is a no-op');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_uniform_readback_does_not_depend_on_current_program_or_reuse_mutable_names() {
    check(&format!(
        "{SETUP}{}",
        r#"
        const p=link('uniform vec4 color;void main(){gl_FragColor=color;}');
        let reads=0;
        const l=gl.getUniformLocation(p,{toString(){reads++;return 'color'}});
        gl.useProgram(p);gl.uniform4f(l,0.25,0.5,0.75,1);gl.useProgram(null);
        for(let i=0;i<128;i++) assert(String(gl.getUniform(p,l))==='0.25,0.5,0.75,1','native cached type');
        assert(reads===1 && gl.getError()===0,'location conversion is not repeated by queries');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
