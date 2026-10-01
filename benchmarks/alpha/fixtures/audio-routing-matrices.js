(() => {
  const {check, near, context, bufferSource} = AudioRoutingChecks;
  const s = Math.SQRT1_2;
  // Independent specification matrices, not imports of Breeze's mixer.
  const matrices = [
    [1,1,[[1]]], [1,2,[[1],[1]]], [1,4,[[1],[1],[0],[0]]],
    [1,6,[[0],[0],[1],[0],[0],[0]]],
    [2,1,[[0.5,0.5]]], [2,2,[[1,0],[0,1]]],
    [2,4,[[1,0],[0,1],[0,0],[0,0]]],
    [2,6,[[1,0],[0,1],[0,0],[0,0],[0,0],[0,0]]],
    [4,1,[[0.25,0.25,0.25,0.25]]],
    [4,2,[[0.5,0,0.5,0],[0,0.5,0,0.5]]],
    [4,4,[[1,0,0,0],[0,1,0,0],[0,0,1,0],[0,0,0,1]]],
    [4,6,[[1,0,0,0],[0,1,0,0],[0,0,0,0],[0,0,0,0],[0,0,1,0],[0,0,0,1]]],
    [6,1,[[s,s,1,0,0.5,0.5]]],
    [6,2,[[1,0,s,0,s,0],[0,1,s,0,0,s]]],
    [6,4,[[1,0,s,0,0,0],[0,1,s,0,0,0],[0,0,0,0,1,0],[0,0,0,0,0,1]]],
    [6,6,[[1,0,0,0,0,0],[0,1,0,0,0,0],[0,0,1,0,0,0],
      [0,0,0,1,0,0],[0,0,0,0,1,0],[0,0,0,0,0,1]]]
  ];
  for (const [inputs, outputs, matrix] of matrices) {
    check(`speaker matrix ${inputs}->${outputs}`, async () => {
      const ctx = context(outputs, 128);
      ctx.destination.channelInterpretation = 'discrete';
      const source = bufferSource(ctx, inputs, 128, (samples, channel) => {
        samples[channel * 2] = 1;
      });
      const gain = new GainNode(ctx, {channelCount: outputs, channelCountMode: 'explicit'});
      source.connect(gain).connect(ctx.destination);
      source.start();
      const result = await ctx.startRendering();
      for (let channel = 0; channel < outputs; channel++)
        for (let frame = 0; frame < result.length; frame++) {
          const expected = frame % 2 === 0 && frame < inputs * 2 ?
            matrix[channel][frame / 2] : 0;
          near(result.getChannelData(channel)[frame], expected, 'speaker impulse basis');
        }
    });
  }
  for (const inputs of [1,2,3,4,5,6,7,8]) for (const outputs of [1,2,4,6]) {
    check(`discrete matrix ${inputs}->${outputs}`, async () => {
      const ctx = context(outputs, 128);
      ctx.destination.channelInterpretation = 'discrete';
      const source = bufferSource(ctx, inputs, 128, (samples, channel) => {
        samples[channel * 2] = 1;
      });
      const gain = new GainNode(ctx, {channelCount: outputs, channelCountMode: 'explicit',
        channelInterpretation: 'discrete'});
      source.connect(gain).connect(ctx.destination);
      source.start();
      const result = await ctx.startRendering();
      for (let channel = 0; channel < outputs; channel++)
        for (let frame = 0; frame < result.length; frame++)
          near(result.getChannelData(channel)[frame],
            channel < inputs && frame === channel * 2 ? 1 : 0, 'discrete impulse basis');
    });
  }
  check('AudioParam 5.1 downmix omits LFE', async () => {
    const ctx = context(1, 128);
    const carrier = new ConstantSourceNode(ctx);
    const modulation = bufferSource(ctx, 6, 128, (samples, channel) => {
      samples.fill(channel === 2 ? 0.5 : channel === 3 ? 1 : 0);
    });
    const gain = new GainNode(ctx, {gain: 0});
    carrier.connect(gain).connect(ctx.destination);
    modulation.connect(gain.gain);
    carrier.start(); modulation.start();
    const result = await ctx.startRendering();
    for (const value of result.getChannelData(0)) near(value, 0.5, 'surround parameter mono');
  });
  check('merger speaker mono downmix', async () => {
    const ctx = context(2, 128);
    const source = bufferSource(ctx, 2, 128, (samples, channel) => samples.fill(channel ? 0.75 : 0.25));
    const merger = new ChannelMergerNode(ctx, {numberOfInputs: 2});
    source.connect(merger, 0, 1);
    merger.connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    for (let i = 0; i < result.length; ++i) {
      near(result.getChannelData(0)[i], 0, 'unconnected merger input');
      near(result.getChannelData(1)[i], 0.5, 'merger mono average');
    }
  });
})();
