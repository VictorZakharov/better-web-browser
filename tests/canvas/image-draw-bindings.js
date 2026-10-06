function testCanvasImageDrawBindings(createCanvas) {
    const target=createCanvas(32,32), c=target.getContext('2d');
    const source=createCanvas(2,2), s=source.getContext('2d');
    s.fillStyle='red';s.fillRect(0,0,2,2);
    const equal=(a,b,label)=>{if(a!==b)throw Error(label+': '+a+' != '+b);};
    const throws=(run,name,label)=>{try{run();}catch(e){equal(e.name,name,label);return;}throw Error(label+' did not throw');};
    for(const count of [0,1,2,4,6,7,8])
        throws(()=>c.drawImage(...[source,0,0,2,2,0,0,32,32].slice(0,count)), 'TypeError', 'arity '+count);
    let touched=0;
    const extra={valueOf(){touched++;throw Error('unused argument');}};
    c.drawImage(source,0,0,2,2,0,0,32,32,extra);
    equal(touched,0,'surplus conversion');
    equal(Array.from(c.getImageData(8,8,1,1).data).join(),'255,0,0,255','surplus draw');
    for(const invalid of [1n,Symbol('coordinate')])
        throws(()=>c.drawImage(source,invalid,0),'TypeError','IDL ToNumber');
    const poison={valueOf(){touched++;throw Error('must not convert');}};
    throws(()=>c.drawImage({},poison,0),'TypeError','source interface precedes coordinates');
    equal(touched,0,'source brand conversion order');
    throws(()=>CanvasRenderingContext2D.prototype.drawImage.call({},source,poison,0),
        'TypeError','context brand precedes arguments');
    equal(touched,0,'context brand conversion order');
    const order=[];
    c.drawImage(source,...Array.from({length:8},(_,i)=>({valueOf(){order.push(i);return [0,0,2,2,0,0,32,32][i];}})));
    equal(order.join(),'0,1,2,3,4,5,6,7','left-to-right conversion');
    // A numeric conversion may update source pixels. Snapshot only afterward.
    c.drawImage(source,{valueOf(){s.fillStyle='blue';s.fillRect(0,0,2,2);return 0;}},0,32,32);
    equal(Array.from(c.getImageData(8,8,1,1).data).join(),'0,0,255,255','source snapshot after conversion');
    // Self-draw with a source layer follows the same order, including copy.
    c.globalCompositeOperation='copy';
    c.drawImage(target,{valueOf(){c.globalCompositeOperation='source-over';c.fillStyle='green';
        c.fillRect(0,0,32,32);c.globalCompositeOperation='copy';return 0;}},0,32,32);
    equal(Array.from(c.getImageData(8,8,1,1).data).join(),'0,128,0,255','self source-layer snapshot after conversion');
    const empty=createCanvas(0,0);
    c.drawImage(empty,NaN,0); // nonfinite drawing algorithm returns before usability
    throws(()=>c.drawImage(empty,0,0),'InvalidStateError','empty source usability');
    // Native interface branding survives prototype changes; duck typing cannot
    // grant untrusted objects access to canvas/image host storage.
    const saved=Object.getPrototypeOf(source);
    Object.setPrototypeOf(source,{});
    c.globalCompositeOperation='source-over';c.drawImage(source,0,0,32,32);
    Object.setPrototypeOf(source,saved);
    equal(Array.from(c.getImageData(8,8,1,1).data).join(),'0,0,255,255','private source interface');
    return {passed:true};
}
