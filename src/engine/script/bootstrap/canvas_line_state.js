    // HTML line-state algorithms run after Web IDL argument conversion. In
    // particular, sequence numbers convert between iterator steps, not afterward.
    const canvasLineApply=Reflect.apply,canvasLineDefine=Object.defineProperty;
    const canvasLineFinite=Number.isFinite;
    const canvasLineOwn=(array,index,value)=>canvasLineDefine(array,index,
        {value,writable:true,enumerable:true,configurable:true});
    const canvasLineObject=value=>value!==null&&(typeof value==='object'||typeof value==='function');
    const canvasLineAttribute=(name,slot,convert,valid)=>canvasLineDefine(CanvasRenderingContext2D.prototype,name,{
        enumerable:true,configurable:true,
        get(){canvasImageDataContext(this);return this[slot];},
        set(value){canvasImageDataContext(this);value=convert(value);if(valid(value))this[slot]=value;}
    });
    canvasLineAttribute('lineWidth','__lineWidth',value=>+value,value=>canvasLineFinite(value)&&value>0);
    canvasLineAttribute('miterLimit','__miterLimit',value=>+value,value=>canvasLineFinite(value)&&value>0);
    canvasLineAttribute('lineDashOffset','__dashOffset',value=>+value,canvasLineFinite);
    canvasLineAttribute('lineCap','__lineCap',value=>`${value}`,
        value=>value==='butt'||value==='round'||value==='square');
    canvasLineAttribute('lineJoin','__lineJoin',value=>`${value}`,
        value=>value==='round'||value==='bevel'||value==='miter');
    const canvasLineSequence=value=>{
        if(!canvasLineObject(value))throw new TypeError('Dash segments must be an iterable object');
        const method=value[Symbol.iterator];
        if(typeof method!=='function')throw new TypeError('Dash segments must be iterable');
        const iterator=canvasLineApply(method,value,[]);
        if(!canvasLineObject(iterator))throw new TypeError('Invalid dash iterator');
        const next=iterator.next,result=[];
        // Web IDL sequence conversion does not IteratorClose on a failed numeric
        // conversion. Preserve original exceptions and do not call author return.
        for(;;){
            const step=canvasLineApply(next,iterator,[]);
            if(!canvasLineObject(step))throw new TypeError('Invalid dash iterator result');
            if(step.done)return result;
            canvasLineOwn(result,result.length,+step.value);
        }
    };
    CanvasRenderingContext2D.prototype.setLineDash=function setLineDash(segments){
        canvasImageDataContext(this);
        if(arguments.length<1)throw new TypeError('setLineDash requires segments');
        const values=canvasLineSequence(segments);
        // Complete every conversion before validating or replacing drawing state.
        for(let index=0;index<values.length;index++)
            if(!canvasLineFinite(values[index])||values[index]<0)return;
        const length=values.length;
        if(length%2)for(let index=0;index<length;index++)canvasLineOwn(values,length+index,values[index]);
        this.__lineDash=values;
    };
    CanvasRenderingContext2D.prototype.getLineDash=function getLineDash(){
        canvasImageDataContext(this);
        const copy=[];
        for(let index=0;index<this.__lineDash.length;index++)canvasLineOwn(copy,index,this.__lineDash[index]);
        return copy;
    };
