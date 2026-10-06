// Shared Window and Worker checks: spacing changes shaping and pixels, not just getters.
function testCanvasTextSpacing(make) {
    const assert = (value, label) => { if (!value) throw Error(label); };
    const close = (a,b,label) => assert(Math.abs(a-b)<0.03, `${label}: ${a} versus ${b}`);
    const canvas = make(420,120), c = canvas.getContext('2d');
    assert(c.letterSpacing === '0px' && c.wordSpacing === '0px', 'initial spacing');
    for (const name of ['letterSpacing','wordSpacing']) {
        const descriptor = Object.getOwnPropertyDescriptor(CanvasRenderingContext2D.prototype,name);
        assert(descriptor.enumerable && descriptor.configurable, 'IDL spacing descriptor');
        let converted = false, rejected = false;
        try { descriptor.set.call({}, {toString(){converted=true;return '1px';}}); }
        catch(e) { rejected=e instanceof TypeError; }
        assert(rejected && !converted,'receiver before DOMString');
        rejected = false;
        try { c[name] = Symbol(); } catch(e) { rejected=e instanceof TypeError; }
        assert(rejected,'Symbol spacing rejects');
        const hints=[];
        c[name] = {[Symbol.toPrimitive](hint){hints.push(hint);return '+2.50PX';}};
        assert(hints.join() === 'string' && c[name] === '2.5px','canonical specified spacing');
        for(const invalid of ['normal','auto','inherit','5%','calc(10% - 10%)',
            '5','1 px','var(--unknown)','NaNpx','1px!important']) {
            c[name]=invalid;
            assert(c[name]==='2.5px',`invalid ignored: ${invalid}`);
        }
        c[name]='0px';
    }
    c.font='20px Arial';
    const a = c.measureText('ABC').width;
    c.letterSpacing='4px';
    close(c.measureText('ABC').width, a+12,'one spacing advance per character');
    let conversions=0;
    close(c.measureText({toString(){conversions++;c.letterSpacing='5px';return 'ABC';}}).width,
        a+15,'measureText conversion precedes selecting current shaping state');
    assert(conversions===1,'measureText converts once');
    c.letterSpacing='-2px';
    close(c.measureText('ABC').width,a-6,'negative letter spacing');
    c.letterSpacing='0px';
    const combining=c.measureText('a\u0301B').width;
    c.letterSpacing='3px';
    close(c.measureText('a\u0301B').width,combining+6,'combining mark is not another letter');
    c.letterSpacing='0px';
    const words=c.measureText('A B C').width;
    c.wordSpacing='7px';
    close(c.measureText('A B C').width,words+14,'word spacing applies to spaces');
    close(c.measureText('A\tB\nC').width,words+14,'Canvas whitespace normalization');
    c.letterSpacing='2px';
    close(c.measureText('A B C').width,words+14+10,'letter and word spacing combine');
    c.save(); c.letterSpacing='4em'; c.wordSpacing='9px'; c.restore();
    assert(c.letterSpacing==='2px' && c.wordSpacing==='7px','spacing save/restore');
    c.wordSpacing='0px'; c.letterSpacing='0.5em';
    close(c.measureText('ABC').width,a+30,'em uses Canvas font');
    c.font='40px Arial';
    c.letterSpacing='0px'; const large=c.measureText('ABC').width;
    c.letterSpacing='0.5em';
    close(c.measureText('ABC').width,large+60,'em recomputed after font change');
    c.letterSpacing='calc(0.5em + 2px)';
    close(c.measureText('ABC').width,large+66,'mixed calc length');
    c.letterSpacing='1pc';
    close(c.measureText('ABC').width,large+48,'absolute CSS length');
    c.letterSpacing='0px'; c.font='24px Arial';
    const inkRight = () => {
        const pixels=c.getImageData(0,0,420,120).data;
        let right=-1;
        for(let y=0;y<120;y++) for(let x=0;x<420;x++)
            if(pixels[(y*420+x)*4+3]) right=Math.max(right,x);
        return right;
    };
    c.fillText('ABC',10,40); const before=inkRight();
    c.clearRect(0,0,420,120); c.letterSpacing='10px'; c.fillText('ABC',10,40);
    assert(inkRight()>=before+18,'fillText actually moves glyphs');
    c.clearRect(0,0,420,120); c.letterSpacing='0px'; c.strokeText('ABC',10,40);
    const strokeBefore=inkRight();
    c.clearRect(0,0,420,120); c.letterSpacing='10px'; c.strokeText('ABC',10,40);
    assert(inkRight()>=strokeBefore+18,'strokeText actually moves glyphs');
    canvas.width=canvas.width;
    assert(c.letterSpacing==='0px' && c.wordSpacing==='0px','resize resets spacing');
}
