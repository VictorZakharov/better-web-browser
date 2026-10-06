function testPrivateCanvasDrawingState(makeCanvas) {
    const assert=(value,label)=>{if(!value)throw Error(label);};
    const poisoned=makeCanvas(40,40), reference=makeCanvas(40,40);
    const context=poisoned.getContext('2d'), plain=reference.getContext('2d');
    const slots=['fill','globalAlpha','compositeOperation','stroke','lineWidth','lineCap','lineJoin',
        'miterLimit','lineDash','dashOffset','imageSmoothingEnabled','imageSmoothingQuality',
        'transform','clipBits','shadowColor','shadowBlur','shadowOffsetX','shadowOffsetY','filter',
        'filterOperations','font','fontSpec','textAlign','textBaseline','direction','path','stack',
        'reset','paintRect'];
    for(const name of slots){
        assert(!Object.hasOwn(context,'__'+name),'internal state is not an own property '+name);
        Object.defineProperty(context,'__'+name,{
            get(){throw Error('author read '+name);},set(){throw Error('author write '+name);}
        });
    }
    const draw=target=>{
        target.fillStyle='red';target.fillRect(0,0,40,40);
        target.save();target.translate(2,3);target.globalAlpha=.5;
        target.beginPath();target.roundRect(1,1,15,12,[2,3]);target.clip();
        target.fillStyle='blue';target.fillRect(0,0,30,30);target.restore();
        target.save();target.strokeStyle='white';target.lineWidth=2.5;
        target.setLineDash([3,2]);target.lineDashOffset=.5;target.lineJoin='round';target.lineCap='square';
        target.beginPath();target.moveTo(3,25);target.bezierCurveTo(10,20,20,35,35,25);target.stroke();
        target.restore();target.shadowColor='rgba(0,255,0,.5)';target.shadowBlur=2;
        target.shadowOffsetX=1;target.fillStyle='white';target.fillRect(25,2,5,5);
        target.shadowColor='transparent';target.filter='drop-shadow(1px 1px 1px blue)';
        target.fillRect(24,12,3,3);target.filter='none';
        const source=new OffscreenCanvas(2,2), sourceContext=source.getContext('2d');
        sourceContext.fillStyle='yellow';sourceContext.fillRect(0,0,2,2);
        target.imageSmoothingEnabled=false;target.drawImage(source,5,32,12,5);
        if(typeof target.fillText==='function'){
            target.font='10px sans-serif';target.textAlign='left';target.textBaseline='alphabetic';
            target.direction='ltr';target.fillText('A',25,36);
        }
    };
    draw(context);draw(plain);
    const actual=context.getImageData(0,0,40,40).data, expected=plain.getImageData(0,0,40,40).data;
    assert(actual.every((value,index)=>value===expected[index]),'author expandos cannot replace renderer state');
    context.reset();assert(context.fillStyle==='#000000'&&context.globalAlpha===1,'private reset restores defaults');
    assert(context.getImageData(0,0,1,1).data[3]===0,'private reset clears bitmap');
    context.fillStyle='red';context.fillRect(0,0,40,40);
    poisoned.width=40;
    assert(context.fillStyle==='#000000'&&context.getImageData(0,0,1,1).data[3]===0,
        'dimension reset does not invoke author reset hook');
    assert(!Object.hasOwn(CanvasRenderingContext2D.prototype,'__reset')&&
        !Object.hasOwn(CanvasRenderingContext2D.prototype,'__paintRect'),'internal methods are not public hooks');
    let error;
    try{new CanvasRenderingContext2D(poisoned);}catch(value){error=value;}
    assert(error?.name==='TypeError','2D context constructor is illegal');
    return {passed:true};
}

function testCanvasContextReceiverOrder(makeCanvas) {
    const context=makeCanvas(16,16).getContext('2d');
    const prototype=CanvasRenderingContext2D.prototype;
    let reads=0;
    const argument={valueOf(){reads++;throw Error('argument value');},toString(){reads++;throw Error('argument string');}};
    for(const name of Object.getOwnPropertyNames(prototype)){
        if(name==='constructor')continue;
        const descriptor=Object.getOwnPropertyDescriptor(prototype,name);
        for(const member of ['get','set','value']){
            const method=descriptor[member];if(typeof method!=='function')continue;
            let error;
            try{method.call({},argument,argument,argument,argument,argument,argument);}catch(value){error=value;}
            if(error?.name!=='TypeError'||reads!==0)throw Error(name+' receiver did not precede conversion');
        }
        if(!descriptor.configurable||!descriptor.enumerable)throw Error(name+' IDL descriptor');
    }
    context.fillStyle='red';context.save();context.fillStyle='blue';context.restore();
    if(context.fillStyle!=='#ff0000')throw Error('receiver wrapper changed valid context behavior');
    return {passed:true};
}

function testCanvasPrivateSerialization(makeCanvas) {
    const canvas=makeCanvas(40,40), context=canvas.getContext('2d');
    const source=new OffscreenCanvas(2,2), image=source.getContext('2d');
    image.fillStyle='blue';image.fillRect(0,0,2,2);
    let calls=0;
    const stringify=JSON.stringify;
    Object.prototype.toJSON=function(){calls++;throw Error('private record exposed');};
    Array.prototype.toJSON=function(){calls++;throw Error('private geometry exposed');};
    JSON.stringify=()=>{throw Error('author serializer invoked');};
    try{
        context.beginPath();context.moveTo(2,2);context.quadraticCurveTo(16,1,30,15);
        context.bezierCurveTo(20,25,10,30,2,20);context.closePath();
        context.fillStyle='red';context.fill();
        context.strokeStyle='white';context.lineWidth=2;context.stroke();
        context.beginPath();context.arc(20,20,8,0,Math.PI);context.stroke();
        context.drawImage(source,0,0,20,20);
        if(!context.isPointInStroke(20,28))throw Error('private stroke query geometry changed');
        if(context.getImageData(5,5,1,1).data[2]!==255)throw Error('private image payload changed');
    }finally{
        delete Object.prototype.toJSON;delete Array.prototype.toJSON;JSON.stringify=stringify;
    }
    if(calls!==0)throw Error('Canvas geometry reached author toJSON');
    // A retained path still has all normal operations after wire serialization:
    // protecting the payload must not strip prototypes from live geometry.
    context.lineTo(3,3);context.stroke();context.fill();
    return {passed:true};
}
