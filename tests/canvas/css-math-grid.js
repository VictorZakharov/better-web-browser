'use strict';
function runCSSMathGrid() {
    const rows=[], failures=[];
    const check=(name,actual,expected)=>{
        const passed=Number.isFinite(actual) && Math.abs(actual-expected)<=.02;
        rows.push([actual,expected,Number(passed)]);
        if(!passed) failures.push(name+': '+actual+' != '+expected);
    };
    const cases=[
        ['grid-template-columns:repeat(calc(1 + 2),50px);column-gap:10px',
            f=>[[0,0,50,20],[60,0,50,20],[120,0,50,20]]],
        ['grid-template-columns:calc(120px / 2) min(30%,80px) hypot(3px,4px)',
            f=>[[0,0,60,20],[60,0,80,20],[140,0,5,20]]],
        ['grid-template-columns:repeat(sqrt(4),max(1em,20px) 30px)',
            f=>[[0,0,Math.max(f,20),20],[Math.max(f,20),0,30,20],
                [Math.max(f,20)+30,0,Math.max(f,20),20]]],
        ['grid-template-columns:0fr .5fr 1fr',
            f=>[[0,0,0,20],[0,0,100,20],[100,0,200,20]]],
        ['grid-template-columns:50px 50px 50px;grid-template-columns:minmax(1fr,2fr)',
            f=>[[0,0,50,20],[50,0,50,20],[100,0,50,20]]],
        ['grid-template-rows:repeat(sqrt(9),20px);row-gap:5px',
            f=>[[0,0,300,20],[0,25,300,20],[0,50,300,20]]],
        ['grid-template-columns:minmax(max(1em,10%),1fr) 50px 50px',
            f=>[[0,0,Math.max(f,200),20],[Math.max(f,200),0,50,20],
                [Math.max(f,200)+50,0,50,20]]],
        ['grid-template-columns:max(10px,20%) 1fr 1fr;column-gap:10px',
            f=>[[0,0,60,20],[70,0,110,20],[190,0,110,20]]],
        ['grid-template-rows:minmax(max(20px,40px),calc(10px + 10px)) 20px 20px',
            f=>[[0,0,300,20],[0,40,300,20],[0,60,300,20]]]
    ];
    for(const font of [16,40,220]) {
        for(let index=0;index<cases.length;index++) {
            const grid=document.createElement('main');
            grid.style.cssText='position:absolute;left:40px;top:40px;display:grid;width:300px;'+
                'margin:0;padding:0;border:0;font-size:'+font+'px;'+cases[index][0];
            for(let child=0;child<3;child++) {
                const element=document.createElement('div');
                element.style.cssText='height:20px;min-width:0;margin:0;padding:0;border:0;background:blue';
                grid.appendChild(element);
            }
            document.body.appendChild(grid);
            const origin=grid.getBoundingClientRect(),expected=cases[index][1](font);
            for(let child=0;child<3;child++) {
                const rect=grid.children[child].getBoundingClientRect();
                const actual=[rect.left-origin.left,rect.top-origin.top,rect.width,rect.height];
                for(let field=0;field<4;field++)
                    check(index+'/'+font+'/'+child+'/'+field,actual[field],expected[child][field]);
            }
            grid.remove();
        }
    }
    const results=document.getElementById('results');
    for(let start=0;start<rows.length;start+=8)
        results.setAttribute('data-rows-'+start/8,JSON.stringify(rows.slice(start,start+8)));
    results.setAttribute('data-count',String(rows.length));
    results.setAttribute('data-failed',String(failures.length));
    results.setAttribute('data-done','true');
    results.textContent=failures.join('\n') || rows.length+' actual Grid rectangle checks passed';
    return failures;
}
