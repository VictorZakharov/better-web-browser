(() => {
  const {check, equal, near, context, bufferSource} = AudioRoutingChecks;
  check('delay feedback and minimum quantum', async () => {
    const ctx = context(1, 640);
    const source = bufferSource(ctx, 1, 1, samples => { samples[0] = 1; });
    const delay = new DelayNode(ctx, {maxDelayTime: 0.01, delayTime: 0});
    const feedback = new GainNode(ctx, {gain: 0.5});
    source.connect(delay);
    delay.connect(feedback).connect(delay);
    delay.connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    for (let i = 0; i < result.length; ++i)
      near(result.getChannelData(0)[i], i > 0 && i % 128 === 0 ?
        Math.pow(0.5, i / 128 - 1) : 0, 'feedback echo at ' + i);
  });
  check('zero-delay cycle muting preserves independent signal', async () => {
    const ctx = context(1, 256);
    const source = new ConstantSourceNode(ctx, {offset: 0.25});
    const a = ctx.createGain(), b = ctx.createGain();
    equal(a.connect(b), b, 'connect return value');
    equal(b.connect(a), a, 'legal cycle connection');
    source.connect(a); b.connect(ctx.destination);
    source.connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    for (const value of result.getChannelData(0)) near(value, 0.25, 'cycle isolation');
  });
  check('AudioParam self-cycle muting', async () => {
    const ctx = context(1, 256);
    const source = new ConstantSourceNode(ctx);
    const gain = ctx.createGain();
    source.connect(gain).connect(ctx.destination);
    gain.connect(gain.gain); source.start();
    const result = await ctx.startRendering();
    for (const value of result.getChannelData(0)) near(value, 0, 'parameter cycle');
  });
  check('disconnected feedback advances before connection', async () => {
    const ctx = context(1, 512);
    const source = bufferSource(ctx, 1, 1, samples => { samples[0] = 1; });
    const delay = new DelayNode(ctx, {delayTime: 128 / 8000});
    const feedback = new GainNode(ctx, {gain: 0.5});
    source.connect(delay);
    delay.connect(feedback).connect(delay); source.start();
    const suspension = ctx.suspend(256 / 8000);
    suspension.then(() => { delay.connect(ctx.destination); return ctx.resume(); });
    const result = await ctx.startRendering();
    for (let i = 0; i < result.length; ++i)
      near(result.getChannelData(0)[i], i === 256 ? 0.5 : i === 384 ? 0.25 : 0,
        'disconnected delay history at ' + i);
  });
})();
