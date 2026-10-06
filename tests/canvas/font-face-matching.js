async function testFontFaceMatching(bytes) {
    const assert=(v,m)=>{if(!v)throw Error(m);};
    const set=typeof document==='object'?document.fonts:fonts;
    set.clear();
    const regular=new FontFace("'Comma, Family'",bytes,{weight:'400'});
    const italic=new FontFace("'Comma, Family'",bytes,{weight:'700',style:'italic'});
    const second=new FontFace('Second Family',bytes);
    await Promise.all([regular.load(),italic.load(),second.load()]);
    set.add(regular);set.add(italic);set.add(second);
    const queried=await set.load("italic 700 16px 'Comma, Family', 'Second Family'",'ABC');
    assert(queried.length===2&&queried[0]===italic&&queried[1]===second,
        'quoted comma, all listed families, and style/weight matching');
    assert((await set.load("16px 'Missing', 'Comma, Family'",'A'))[0]===regular,
        'missing first family does not hide subsequent listed face');
    assert(set.check("16px 'Comma, Family'"),'registered quoted family checks loaded');
    const pending=new FontFace('Pending Family','url(/not-requested.ttf)');
    set.add(pending);
    assert(!set.check("16px 'Pending Family'",'A'),'unloaded matching face requires a load');
    assert(set.check("16px 'Pending Family'",''),'empty text does not load a matching face');
    assert((await set.load("16px 'Pending Family'",'')).length===0,'empty load resolves no faces');
    assert(pending.status==='unloaded','query did not request pending font');
    const fallback300=new FontFace('Weight Family',bytes,{weight:'300'});
    const fallback500=new FontFace('Weight Family',bytes,{weight:'500'});
    await Promise.all([fallback300.load(),fallback500.load()]);
    set.add(fallback300);set.add(fallback500);
    const matching=await set.load("400 16px 'Weight Family'",'A');
    assert(matching.length===1&&matching[0]===fallback500,
        'CSS weight search from 400 prefers 500 before 300');
    for(const invalid of ['inherit','initial','unset','16px','var(--font)','unknown 16px Family']) {
        let thrown=false;try{set.check(invalid);}catch(error){thrown=error.name==='SyntaxError';}
        assert(thrown,'check rejects invalid shorthand '+invalid);
        let rejected=false;await set.load(invalid).catch(error=>{rejected=error.name==='SyntaxError';});
        assert(rejected,'load rejects invalid shorthand '+invalid);
    }
    const context=(typeof document==='object'?document.createElement('canvas'):
        new OffscreenCanvas(100,50)).getContext('2d');
    context.font="20px 'Comma, Family'";
    assert(context.measureText('ABC').width===60,'quoted loaded face reaches actual Canvas provider');
    set.clear();return 'passed';
}
