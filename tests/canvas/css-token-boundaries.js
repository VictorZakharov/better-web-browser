'use strict';
function runCSSTokenBoundaries() {
    const rows=[], failures=[];
    const check=(name,actual,expected)=>{
        const passed=Number.isFinite(actual) && Math.abs(actual-expected)<=.02;
        rows.push([actual,expected,Number(passed)]);
        if(!passed) failures.push(name+': '+actual+' != '+expected);
    };
    // Null means invalid grammar. At parse time the old declaration survives;
    // after var() substitution the winning declaration computes to unset.
    const cases=[
        ['width','10px',10],
        ['width','10px/**/',10],
        ['width','10/**/px',null],
        ['width','10 px',null],
        ['width','calc/**/(10px)',null],
        ['width','calc(5px /**/ + /**/ 5px)',10],
        ['width','calc(2/**/ * /**/5px)',10],
        ['width','max(10px/**/,20px)',20],
        ['width','sqrt(4)',null],
        ['width','calc(sqrt(4) * 5px)',10],
        ['width','25%',75],
        ['width','25/**/%',null],
        ['opacity','.5',.5],
        ['opacity','.5/**/',.5],
        ['opacity','25%',.25],
        ['opacity','25/**/%',null],
        ['opacity','25 %',null],
        ['opacity','+ .5',null],
        ['opacity','0x1',null],
        ['opacity','calc(25% * 2)',.5],
        ['opacity','calc(.25 /**/ + /**/ .25)',.5],
        ['opacity','sqrt(.25)',.5],
        ['opacity','calc(1px)',null],
        ['opacity','NaN',null]
    ];
    const baseline='width:30px;height:20px;opacity:.7;margin:0;padding:0;border:0;';
    const rule=document.createElement('style');
    rule.textContent=cases.map(([property,value],index)=>
        '.token-probe-sheet-'+index+'{'+baseline+property+':'+value+'}').join('\n');
    document.head.appendChild(rule);
    for(const mode of ['inline','sheet','variable']) {
        for(let index=0;index<cases.length;index++) {
            const [property,value,valid]=cases[index];
            const wrapper=document.createElement('main');
            wrapper.style.cssText='position:absolute;left:40px;top:40px;width:300px';
            const element=document.createElement('div');
            const className='token-probe-'+mode+'-'+index;
            element.className=className;
            if(mode!=='sheet') {
                element.style.cssText=baseline+(mode==='variable'
                    ? '--value:'+value+';'+property+':var(--value)'
                    : property+':'+value);
            }
            wrapper.appendChild(element);
            document.body.appendChild(wrapper);
            const expected=valid!==null ? valid : mode==='variable'
                ? property==='width' ? 300 : 1
                : property==='width' ? 30 : .7;
            const actual=property==='width' ? element.getBoundingClientRect().width
                : Number(getComputedStyle(element).opacity);
            check(mode+'/'+index,actual,expected);
            // The unaffected declaration must remain intact too.
            check(mode+'/'+index+'/height',element.getBoundingClientRect().height,20);
            wrapper.remove();
        }
    }
    rule.remove();
    const results=document.getElementById('results');
    for(let start=0;start<rows.length;start+=8)
        results.setAttribute('data-rows-'+start/8,JSON.stringify(rows.slice(start,start+8)));
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    results.setAttribute('data-done','true');
    results.textContent=failures.join('\n') || rows.length+' actual CSS token checks passed';
    return failures;
}
