// Browser-owned acceptance scenes; no third-party source or shader snapshots.
// The caller supplies an unchanged Three.js build (MIT) to exercise actual APIs.
export async function run(THREE, canvas, results) {
    const renderer = new THREE.WebGLRenderer({canvas, antialias:false});
    const gl = renderer.getContext(), rows = [];
    const camera = new THREE.OrthographicCamera(-1,1,1,-1,.1,10);
    camera.position.z = 2;
    renderer.setSize(64,64,false);
    renderer.setClearColor(0,1);
    const release = (...values) => { for (const value of values) value.dispose(); };
    const equal = (actual, expected) => JSON.stringify(actual) === JSON.stringify(expected);
    const pixels = (points = [[32,32]]) => points.map(([x,y]) => {
        const value = new Uint8Array(4);
        gl.readPixels(x,y,1,1,gl.RGBA,gl.UNSIGNED_BYTE,value);
        return Array.from(value);
    });
    const plane = material => {
        const geometry = new THREE.PlaneGeometry(2,2), scene = new THREE.Scene();
        scene.add(new THREE.Mesh(geometry,material));
        return {scene, geometry};
    };
    const check = (name, operation) => {
        let row;
        try {
            const observation = operation();
            row = {name, ...observation, error:gl.getError()};
            row.passed = row.passed && row.error === gl.NO_ERROR;
        } catch (error) {
            row = {name, passed:false, reason:String(error.message || error)};
        }
        rows.push(row);
        const element = document.createElement('pre');
        element.className = 'contract';
        element.setAttribute('data-result', JSON.stringify(row));
        element.textContent = name + ': ' + (row.passed ? 'PASS' : 'FAIL');
        document.body.appendChild(element);
    };
    try {
        check('public WebGL2 and exclusive Canvas mode', () => ({
            passed:gl instanceof WebGL2RenderingContext && gl.canvas === canvas &&
                canvas.getContext('webgl') === null && canvas.getContext('2d') === null,
            revision:THREE.REVISION, version:gl.getParameter(gl.VERSION)
        }));
        check('indexed data-texture quadrants', () => {
            const texture = new THREE.DataTexture(new Uint8Array([
                255,0,0,255, 0,255,0,255, 0,0,255,255, 255,255,0,255]),2,2);
            texture.needsUpdate = true;
            const material = new THREE.MeshBasicMaterial({map:texture});
            const {scene,geometry} = plane(material);
            renderer.render(scene,camera);
            const values = pixels([[16,16],[48,16],[16,48],[48,48]]);
            release(texture,material,geometry);
            return {passed:equal(values,[[255,0,0,255],[0,255,0,255],
                [0,0,255,255],[255,255,0,255]]), pixels:values};
        });
        check('instanced indexed attributes', () => {
            const geometry = new THREE.PlaneGeometry(.7,.7);
            const material = new THREE.MeshBasicMaterial({color:0xffffff});
            const mesh = new THREE.InstancedMesh(geometry,material,2), scene = new THREE.Scene();
            mesh.setMatrixAt(0,new THREE.Matrix4().makeTranslation(-.5,0,0));
            mesh.setMatrixAt(1,new THREE.Matrix4().makeTranslation(.5,0,0));
            mesh.setColorAt(0,new THREE.Color(0xff0000));
            mesh.setColorAt(1,new THREE.Color(0x00ff00));
            scene.add(mesh);renderer.render(scene,camera);
            const values = pixels([[16,32],[48,32],[32,32]]);
            const calls = renderer.info.render.calls;
            release(geometry,material);mesh.dispose();
            return {passed:equal(values,[[255,0,0,255],[0,255,0,255],[0,0,0,255]]) &&
                calls === 1, pixels:values, calls};
        });
        check('multisampled render target', () => {
            const target = new THREE.WebGLRenderTarget(16,16,{samples:4});
            const material = new THREE.MeshBasicMaterial({color:0x0000ff});
            const {scene,geometry} = plane(material);
            renderer.setRenderTarget(target);renderer.render(scene,camera);
            renderer.setRenderTarget(null);
            const value = new Uint8Array(4);
            renderer.readRenderTargetPixels(target,8,8,1,1,value);
            release(target,geometry,material);
            return {passed:equal(Array.from(value),[0,0,255,255]), pixels:[Array.from(value)]};
        });
        check('float render target', () => {
            const target = new THREE.WebGLRenderTarget(16,16,{type:THREE.FloatType});
            const material = new THREE.MeshBasicMaterial({color:0x00ff00});
            const {scene,geometry} = plane(material);
            renderer.setRenderTarget(target);renderer.render(scene,camera);
            renderer.setRenderTarget(null);
            const value = new Float32Array(4);
            renderer.readRenderTargetPixels(target,8,8,1,1,value);
            release(target,geometry,material);
            return {passed:equal(Array.from(value),[0,1,0,1]), pixels:[Array.from(value)]};
        });
        const texturedVolume = (texture, declaration, sample) => {
            texture.format = THREE.RedFormat;texture.needsUpdate = true;
            const material = new THREE.ShaderMaterial({glslVersion:THREE.GLSL3,
                uniforms:{volume:{value:texture}},
                vertexShader:'void main(){gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.);}',
                fragmentShader:'precision highp ' + declaration + ';uniform ' + declaration +
                    ' volume;out vec4 color;void main(){color=vec4(texture(volume,' +
                    sample + ').r,0.,0.,1.);}'});
            const {scene,geometry} = plane(material);
            renderer.render(scene,camera);const values = pixels();
            release(texture,material,geometry);
            return {passed:equal(values,[[192,0,0,255]]), pixels:values};
        };
        check('3D texture sampling', () => texturedVolume(
            new THREE.Data3DTexture(new Uint8Array([64,192]),1,1,2),
            'sampler3D','vec3(.5,.5,.75)'));
        check('array texture sampling', () => texturedVolume(
            new THREE.DataArrayTexture(new Uint8Array([64,192]),1,1,2),
            'sampler2DArray','vec3(.5,.5,1.)'));
        check('lit shadowed standard material', () => {
            renderer.shadowMap.enabled = true;renderer.shadowMap.type = THREE.PCFShadowMap;
            const scene = new THREE.Scene(), light = new THREE.DirectionalLight(0xffffff,2);
            light.position.set(1,1,2);light.castShadow = true;light.shadow.mapSize.set(64,64);
            scene.add(light);scene.add(new THREE.AmbientLight(0xffffff,.5));
            const geometry = new THREE.BoxGeometry(.8,.8,.4);
            const material = new THREE.MeshStandardMaterial({color:0x00ff00});
            const mesh = new THREE.Mesh(geometry,material);
            mesh.castShadow = true;mesh.receiveShadow = true;scene.add(mesh);
            renderer.render(scene,camera);const values = pixels();
            release(geometry,material);light.shadow.dispose();renderer.shadowMap.enabled = false;
            // Permit backend rounding, but not black output or a missing shadow pass.
            return {passed:values[0][1] > 180 && values[0][0] < 40 && values[0][2] < 40 &&
                values[0][3] === 255 && renderer.info.render.calls === 2, pixels:values};
        });
        check('morph-target array texture', () => {
            const material = new THREE.MeshBasicMaterial({color:0xff0000});
            const {scene,geometry} = plane(material);
            const base = geometry.getAttribute('position');
            const shifted = base.clone();
            for (let index = 0; index < shifted.count; index++) shifted.setX(index,base.getX(index)+2);
            geometry.morphAttributes.position = [shifted];
            const mesh = scene.children[0];mesh.updateMorphTargets();mesh.morphTargetInfluences[0] = 1;
            renderer.render(scene,camera);const moved = pixels();
            mesh.morphTargetInfluences[0] = 0;renderer.render(scene,camera);const restored = pixels();
            release(geometry,material);
            return {passed:equal(moved,[[0,0,0,255]]) && equal(restored,[[255,0,0,255]]),
                moved, restored};
        });
        check('half-float MSAA and post-processing', () => {
            const target = new THREE.WebGLRenderTarget(32,32,{samples:4,type:THREE.HalfFloatType});
            const vertexShader = 'varying vec2 uvOut;void main(){uvOut=uv;gl_Position=vec4(position,1.);}';
            const hdr = new THREE.ShaderMaterial({vertexShader,
                fragmentShader:'void main(){gl_FragColor=vec4(4.,.5,0.,1.);}'});
            const {scene,geometry} = plane(hdr);
            renderer.setRenderTarget(target);renderer.render(scene,camera);renderer.setRenderTarget(null);
            const post = new THREE.ShaderMaterial({vertexShader, uniforms:{source:{value:target.texture}},
                fragmentShader:'uniform sampler2D source;varying vec2 uvOut;void main(){'+
                    'vec4 c=texture2D(source,uvOut);gl_FragColor=vec4(c.rgb/4.,c.a);}'});
            scene.children[0].material = post;renderer.render(scene,camera);
            const values = pixels();release(target,geometry,hdr,post);
            return {passed:equal(values,[[255,32,0,255]]), pixels:values};
        });
        check('resize preserves a real drawing buffer', () => {
            renderer.setSize(32,16,false);
            const material = new THREE.MeshBasicMaterial({color:0x0000ff});
            const {scene,geometry} = plane(material);
            renderer.render(scene,camera);const values = pixels([[16,8]]);
            const size = [gl.drawingBufferWidth,gl.drawingBufferHeight];
            release(geometry,material);renderer.setSize(64,64,false);
            return {passed:equal(size,[32,16]) && equal(values,[[0,0,255,255]]), size, pixels:values};
        });
        check('physical material shader', () => {
            const material = new THREE.MeshPhysicalMaterial({color:0x00ff00,
                clearcoat:1,clearcoatRoughness:.2,sheen:1,iridescence:1});
            const {scene,geometry} = plane(material);
            scene.add(new THREE.AmbientLight(0xffffff,2));
            renderer.render(scene,camera);const values = pixels();release(geometry,material);
            return {passed:values[0][1] > 100 && values[0][3] === 255, pixels:values};
        });
    } finally {
        renderer.dispose();
    }
    const failures = rows.filter(row => !row.passed).length;
    results.setAttribute('data-passes', String(rows.length-failures));
    results.setAttribute('data-failures', String(failures));
    results.textContent = JSON.stringify(rows,null,2);
    document.title = failures ? 'failed: Three.js rendering' : 'passed: Three.js rendering';
}
