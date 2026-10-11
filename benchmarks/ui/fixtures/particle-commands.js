// Original, deterministic WebGL 2 primitive workload; no application bundle or
// copied shader. Completion and changed pixels are part of every measurement.
(async()=>{
  const canvas=document.getElementById('field');
  const gl=canvas.getContext('webgl2',{antialias:false,premultipliedAlpha:true,preserveDrawingBuffer:true});
  if(!gl)throw Error('WebGL 2 unavailable');
  const compile=(type,source)=>{
    const shader=gl.createShader(type);gl.shaderSource(shader,source);gl.compileShader(shader);
    if(!gl.getShaderParameter(shader,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(shader));
    return shader;
  };
  const program=gl.createProgram();
  const vertex=`#version 300 es
    in vec2 position;in vec4 color;in vec4 random;
    uniform float phase;uniform sampler2D home;out vec4 tint;
    void main(){
      float gather=smoothstep(0.,1.,phase);
      vec2 destination=position*.25+vec2(-.65,.65);
      vec2 control=(position+destination)*.5+vec2(random.y-.5,random.z-.5)*.5;
      float h=1.-gather;
      vec2 p=h*h*position+2.*h*gather*control+gather*gather*destination;
      gl_Position=vec4(p,0.,1.);gl_PointSize=3.;
      vec4 pixel=texelFetch(home,ivec2(0),0);tint=mix(color,pixel,gather);
    }`;
  const fragment=`#version 300 es
    precision highp float;in vec4 tint;out vec4 pixel;
    void main(){pixel=tint*(1.-smoothstep(.8,1.,length(gl_PointCoord*2.-1.)));}`;
  gl.attachShader(program,compile(gl.VERTEX_SHADER,vertex));
  gl.attachShader(program,compile(gl.FRAGMENT_SHADER,fragment));gl.linkProgram(program);
  if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(program));
  gl.useProgram(program);
  const count=160000,position=new Float32Array(count*2),color=new Uint8Array(count*4),random=new Uint8Array(count*4);
  let seed=12345;
  for(let i=0;i<count;i++){
    position[i*2]=(i%400)/200-1+.005;position[i*2+1]=Math.floor(i/400)/200-1+.005;
    color.set([0,255,0,255],i*4);
    for(let j=0;j<4;j++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;random[i*4+j]=seed>>>24;}
  }
  position[0]=0;position[1]=0;
  const buffers=[];
  for(const [name,data,size,type,normalized]of [['position',position,2,gl.FLOAT,false],
    ['color',color,4,gl.UNSIGNED_BYTE,true],['random',random,4,gl.UNSIGNED_BYTE,true]]){
    const buffer=gl.createBuffer();buffers.push(buffer);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
    gl.bufferData(gl.ARRAY_BUFFER,data,gl.STATIC_DRAW);
    const location=gl.getAttribLocation(program,name);gl.enableVertexAttribArray(location);
    gl.vertexAttribPointer(location,size,type,normalized,0,0);
  }
  const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
  gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,0,0,255]));
  gl.enable(gl.BLEND);gl.blendFunc(gl.ONE,gl.ONE_MINUS_SRC_ALPHA);
  const phase=gl.getUniformLocation(program,'phase');
  const draw=value=>{gl.clear(gl.COLOR_BUFFER_BIT);gl.uniform1f(phase,value);gl.drawArrays(gl.POINTS,0,count);};
  const read=(x,y)=>{const pixel=new Uint8Array(4);gl.readPixels(x,y,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);return pixel.join(',');};
  draw(0);gl.finish();
  if(read(320,180)!=='0,255,0,255')throw Error('starting pixels '+read(320,180));
  draw(1);gl.finish();
  if(read(112,297)!=='255,0,0,255'||read(320,180)!=='0,0,0,0')throw Error('gather pixels');
  const samples=[];
  for(let block=0;block<8;block++){
    const start=performance.now();for(let frame=0;frame<8;frame++)draw(frame/7);
    gl.finish();samples.push((performance.now()-start)/8);
    await new Promise(resolve=>setTimeout(resolve,0));
  }
  const intervals=[],cpu=[];
  if(canvas.hasAttribute('data-animation-frames')){
    const frames=Number(canvas.getAttribute('data-animation-frames'));
    const logo=document.getElementById('logo');
    logo?.animate([{clipPath:'inset(-50% -10% -50% -50%)'},
      {clipPath:'inset(-50% 150% -50% -50%)'}],{duration:1500,easing:'linear',fill:'forwards'});
    let previous;
    for(let frame=0;frame<frames;frame++){
      const now=await new Promise(resolve=>requestAnimationFrame(resolve));
      if(previous!==undefined)intervals.push(now-previous);
      previous=now;
      const start=performance.now();draw(frame/(frames-1));gl.finish();
      cpu.push(performance.now()-start);
    }
    canvas.setAttribute('data-frame-intervals',JSON.stringify(intervals));
    canvas.setAttribute('data-frame-commands',JSON.stringify(cpu));
  }
  if(gl.getError()!==gl.NO_ERROR)throw Error('GL error');
  if(read(112,297)!=='255,0,0,255')throw Error('final result lost');
  canvas.setAttribute('data-completed-ms',JSON.stringify(samples));
  document.getElementById('result').textContent='PASS: 160k normalized point sprites gather; completed draw ms='+samples.join(',');
  document.title='done points ms='+samples.map(value=>value.toFixed(3)).join(',')+';count='+count+';pixels=green/red/clear'+
    (intervals.length?';frameMean='+ (intervals.reduce((a,b)=>a+b,0)/intervals.length).toFixed(3):'');
  document.documentElement.dataset.fixtureReady='true';
})().catch(error=>{document.title='failed '+error.message;throw error;});
