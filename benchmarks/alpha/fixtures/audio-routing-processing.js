(() => {
  const {check, equal, near, context, bufferSource, observe, callbacks} = AudioRoutingChecks;
  check('script processing produces stereo PCM at playbackTime', async () => {
    const ctx = context(2, 2048);
    const source = new ConstantSourceNode(ctx, {offset: 0.25});
    const processor = ctx.createScriptProcessor(256, 2, 2);
    const blocks = [];
    const processing = callbacks();
    processor.onaudioprocess = processing.guard(event => {
      equal(event.inputBuffer.numberOfChannels, 2, 'input layout');
      equal(event.outputBuffer.numberOfChannels, 2, 'output layout');
      equal(event.inputBuffer.length, 256, 'block size');
      for (const value of event.inputBuffer.getChannelData(0)) near(value, 0.25, 'input PCM');
      event.outputBuffer.getChannelData(0).fill(0.5);
      event.outputBuffer.getChannelData(1).fill(-0.25);
      blocks.push(Math.round(event.playbackTime * ctx.sampleRate));
    });
    source.connect(processor).connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    processing.verify();
    observe('script PCM timing', {blocks, firstOutput:
      result.getChannelData(0).findIndex(value => value !== 0)});
    if (!blocks.length) throw Error('no actual processing callbacks');
    for (const frame of blocks) {
      if (frame < 0 || frame + 256 > result.length) continue;
      for (let i = frame; i < frame + 256; i++) {
        near(result.getChannelData(0)[i], 0.5, 'produced left PCM');
        near(result.getChannelData(1)[i], -0.25, 'produced right PCM');
      }
    }
  });
  check('script source zero input side is represented honestly', async () => {
    const ctx = context(1, 2048);
    const processor = ctx.createScriptProcessor(256, 0, 1);
    let calls = 0;
    const processing = callbacks();
    processor.onaudioprocess = processing.guard(event => {
      calls++;
      observe('zero-input callback', {inputChannels: event.inputBuffer?.numberOfChannels ?? null,
        outputChannels: event.outputBuffer?.numberOfChannels ?? null,
        playbackFrame: Math.round(event.playbackTime * ctx.sampleRate)});
      equal(event.inputBuffer.numberOfChannels, 0, 'zero input channels');
      event.outputBuffer.getChannelData(0).fill(0.5);
    });
    processor.connect(ctx.destination);
    const result = await ctx.startRendering();
    processing.verify();
    observe('zero-input result', {calls, firstOutput:
      result.getChannelData(0).findIndex(value => value !== 0)});
    if (!calls || !result.getChannelData(0).some(value => value === 0.5))
      throw Error('script source did not synthesize audio');
  });
  check('script callback output acquisition excludes microtask edits', async () => {
    const ctx = context(1, 2048);
    const processor = ctx.createScriptProcessor(256, 0, 1);
    const processing = callbacks();
    processor.onaudioprocess = processing.guard(event => {
      const output = event.outputBuffer.getChannelData(0);
      output.fill(0.25);
      queueMicrotask(() => output.fill(0.75));
    });
    processor.connect(ctx.destination);
    const result = await ctx.startRendering();
    processing.verify();
    const values = result.getChannelData(0);
    observe('script output ownership', {firstOutput: values.findIndex(value => value !== 0),
      quarter: values.filter(value => value === 0.25).length,
      threeQuarters: values.filter(value => value === 0.75).length});
    if (!values.some(value => value === 0.25) || values.some(value => value === 0.75))
      throw Error('late event buffer edit changed produced PCM');
  });
  check('script processor fixed layout and buffer-size conversion', () => {
    const ctx = context();
    const processor = ctx.createScriptProcessor(4294967552, 1.9, 2);
    equal(processor.bufferSize, 256, 'unsigned bufferSize');
    equal(processor.channelCount, 1, 'unsigned input channels');
    AudioRoutingChecks.throws(() => { processor.channelCount = 2; }, 'NotSupportedError');
    AudioRoutingChecks.throws(() => { processor.channelCountMode = 'max'; }, 'NotSupportedError');
    processor.channelCountMode = 'unknown';
    equal(processor.channelCountMode, 'explicit', 'unknown enum ignored');
  });
  check('analysis uses the mono speaker matrix on the configured input signal', async () => {
    const ctx = context(1);
    const source = bufferSource(ctx, 2, 256, (samples, channel) =>
      samples.fill(channel ? 0.75 : 0.25));
    const analyser = new AnalyserNode(ctx, {fftSize: 32, channelCount: 1,
      channelCountMode: 'explicit', channelInterpretation: 'discrete'});
    source.connect(analyser).connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    for (const value of result.getChannelData(0)) near(value, 0.25, 'pass-through input mix');
    const analysis = new Float32Array(32);
    analyser.getFloatTimeDomainData(analysis);
    for (const value of analysis) near(value, 0.25, 'configured mono analysis');
  });
  for (const interpretation of ['speakers', 'discrete']) {
    check('stereo panner explicit mono ' + interpretation, async () => {
      const ctx = context(2);
      const source = bufferSource(ctx, 2, 256, (samples, channel) =>
        samples.fill(channel ? 0.75 : 0.25));
      const panner = new StereoPannerNode(ctx, {pan: 1, channelCount: 1,
        channelCountMode: 'explicit', channelInterpretation: interpretation});
      source.connect(panner).connect(ctx.destination); source.start();
      const result = await ctx.startRendering();
      for (const value of result.getChannelData(0)) near(value, 0, 'left panning');
      for (const value of result.getChannelData(1))
        near(value, interpretation === 'speakers' ? 0.5 : 0.25, 'mono panning');
    });
  }
  check('convolver uses normative surround input downmix', async () => {
    const ctx = context(2);
    const source = bufferSource(ctx, 6, 256, (samples, channel) => {samples[channel] = 1;});
    const impulse = ctx.createBuffer(1, 1, ctx.sampleRate);
    impulse.getChannelData(0)[0] = 1;
    const convolver = new ConvolverNode(ctx, {buffer: impulse, disableNormalization: true});
    source.connect(convolver).connect(ctx.destination); source.start();
    const result = await ctx.startRendering();
    const expected = [[1,0,Math.SQRT1_2,0,Math.SQRT1_2,0],
      [0,1,Math.SQRT1_2,0,0,Math.SQRT1_2]];
    for (let c = 0; c < 2; c++) for (let i = 0; i < 6; i++)
      near(result.getChannelData(c)[i], expected[c][i], 'convolver input speaker ' + i);
  });
  check('AudioParam mixed layouts downmix each connection before summing', async () => {
    const ctx = context(1);
    const carrier = new ConstantSourceNode(ctx, {offset: 1});
    const stereo = bufferSource(ctx, 2, 256, (samples, channel) => samples.fill(channel ? 0 : 1));
    const surround = bufferSource(ctx, 6, 256, (samples, channel) => samples.fill(channel === 2 ? 1 : 0));
    const gain = new GainNode(ctx, {gain: 0});
    stereo.connect(gain.gain); surround.connect(gain.gain);
    carrier.connect(gain).connect(ctx.destination);
    carrier.start(); stereo.start(); surround.start();
    const result = await ctx.startRendering();
    for (const value of result.getChannelData(0)) near(value, 1.5, 'parameter layout sum');
  });
})();
