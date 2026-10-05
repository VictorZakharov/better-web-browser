function testMatrixContracts(createCanvas) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const canvas=createCanvas();canvas.width=16;canvas.height=8;const c=canvas.getContext('2d');
    const matrix=()=>{const m=c.getTransform();return [m.a,m.b,m.c,m.d,m.e,m.f].join();};
    const error=callback=>{try{callback();return 'none';}catch(e){return e.name;}};
    const errors=[error(()=>c.scale(2)),error(()=>c.translate(2)),error(()=>c.rotate()),
        error(()=>c.transform(1,0,0,1,0)),error(()=>c.setTransform(1,0)),
        error(()=>c.setTransform('matrix(1,0,0,1,0,0)')),error(()=>c.scale(1n,1))];
    assert(errors.every(value=>value==='TypeError'),'numeric/dictionary signature errors '+errors);
    c.setTransform({m11:2,m22:3,m41:1,m42:2});assert(matrix()==='2,0,0,3,1,2','long matrix aliases');
    c.setTransform({a:NaN,m11:NaN});assert(matrix()==='2,0,0,3,1,2','NaN aliases agree but do not change matrix');
    assert(error(()=>c.setTransform({a:2,m11:3}))==='TypeError','alias conflict');
    c.setTransform([2,0,0,2,5,5]);assert(matrix()==='1,0,0,1,0,0','array dictionary, not sequence overload');
    const trace=[],number=(name,value)=>({valueOf(){trace.push(name);return value;}});
    c.transform(number('a',2),number('b',0),number('c',0),number('d',3),number('e',1),number('f',2),Symbol());
    assert(trace.join()==='a,b,c,d,e,f'&&matrix()==='2,0,0,3,1,2','one ordered conversion; extra argument ignored');
    c.translate(Infinity,number('y',1));assert(trace[6]==='y'&&matrix()==='2,0,0,3,1,2','all conversions before nonfinite return');
    c.resetTransform();c.setTransform({m13:1,is2D:false,e:1});assert(matrix()==='1,0,0,1,1,0','unknown dictionary members ignored');
    c.transform=()=>{throw Error('author transform override invoked by another native method');};
    c.scale(2,3);c.translate(1,2);c.rotate(0);assert(matrix()==='2,0,0,3,3,6','methods do not call replaceable public transform');
    c.resetTransform();
    const source=new Path2D();source.rect(0,0,2,2);const target=new Path2D();
    target.addPath(source,{m41:4,m42:2});
    assert(c.isPointInPath(target,5,3)&&!c.isPointInPath(target,1,1),'Path2D long aliases');
    target.addPath(source,{e:Infinity});assert(!c.isPointInPath(target,1,1),'nonfinite path transform is ignored');
    assert(error(()=>target.addPath(source,{a:1,m11:2}))==='TypeError','Path2D conflicting aliases');
    assert(error(()=>Path2D.prototype.addPath.call({},source))==='TypeError','Path2D receiver');
    const samples=[c.isPointInPath(target,5,3),c.isPointInPath(target,1,1)];
    return {errors,samples,trace,matrix:matrix(),passed:true};
}
