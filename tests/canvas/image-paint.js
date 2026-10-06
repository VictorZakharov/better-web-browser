function testCanvasImagePaint(createCanvas) {
    const source = createCanvas(4, 4), context = source.getContext('2d');
    const data = context.createImageData(4, 4);
    for (let y = 0; y < 4; y++) for (let x = 0; x < 4; x++) {
        data.data.set([x * 60, y * 70, (x + y) * 31, [0, 64, 128, 255][(x + y) % 4]],
            (y * 4 + x) * 4);
    }
    context.putImageData(data, 0, 0);
    const modes = ['source-over', 'destination-over', 'lighter', 'multiply', 'screen',
        'overlay', 'darken', 'lighten', 'color-dodge', 'color-burn', 'hard-light',
        'soft-light', 'difference', 'exclusion', 'hue', 'saturation', 'color', 'luminosity'];
    let cases = 0;
    // One large draw uses native sampling. Equivalent small tiles retain the
    // scalar path; both sample the ORIGINAL image, not independently cropped images.
    for (const smooth of [false, true]) for (const mode of modes) {
        for (const transform of [[1,0,0,1,8,8],[1,.25,.125,1,2,3],[0,1,-1,0,40,4]]) {
            const render = tiled => {
                const canvas = createCanvas(48, 48), c = canvas.getContext('2d');
                c.fillStyle = 'rgba(40,80,120,.5)'; c.fillRect(0,0,48,48);
                c.beginPath(); c.rect(4,4,38,38); c.clip();
                c.setTransform(...transform);
                c.imageSmoothingEnabled = smooth;
                c.globalAlpha = .625; c.globalCompositeOperation = mode;
                if (!tiled) c.drawImage(source,0,0,4,4,0,0,32,32);
                else for (let y=0;y<4;y++) for(let x=0;x<4;x++)
                    c.drawImage(source,x,y,1,1,x*8,y*8,8,8);
                return c.getImageData(0,0,48,48).data;
            };
            const native=render(false), scalar=render(true);
            for(let index=0;index<native.length;index++)
                if(native[index]!==scalar[index]) throw Error(
                    `image sampling mismatch ${smooth}/${mode}/${transform} byte ${index}: ${native[index]} != ${scalar[index]}`);
            cases++;
        }
    }
    // Cropped source extending beyond the bitmap must not clamp that whole
    // out-of-image area into the edge texel or clear the untouched backdrop.
    const c=createCanvas(32,32).getContext('2d');
    c.fillStyle='#123456';c.fillRect(0,0,32,32);c.imageSmoothingEnabled=false;
    c.drawImage(source,-4,0,8,4,0,0,32,32);
    if(Array.from(c.getImageData(3,3,1,1).data).join()!=='18,52,86,255')
        throw Error('out-of-image crop changed backdrop');
    // Self drawing snapshots the source before writing overlapping destination.
    const self=createCanvas(32,32), s=self.getContext('2d');
    s.fillStyle='red';s.fillRect(0,0,16,32);s.fillStyle='blue';s.fillRect(16,0,16,32);
    s.imageSmoothingEnabled=false;s.drawImage(self,0,0,16,32,8,0,24,32);
    if(Array.from(s.getImageData(24,8,1,1).data).join()!=='255,0,0,255')
        throw Error('self draw did not snapshot source');
    return {cases:cases+2};
}
