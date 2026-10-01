// Original standards assertions; this runner reports every failure instead of
// aborting after the first missing method. It is shared with focused unit tests.
const AudioRoutingChecks = (() => {
  const cases = [];
  const observations = [];
  const observe = (name, value) => observations.push({name, value});
  const check = (name, operation) => cases.push({ name, operation });
  const equal = (actual, expected, message) => {
    if (actual !== expected) throw Error(message + ': ' + actual + ' != ' + expected);
  };
  const near = (actual, expected, message, tolerance = 0.000001) => {
    if (!Number.isFinite(actual) || Math.abs(actual - expected) > tolerance)
      throw Error(message + ': ' + actual + ' != ' + expected);
  };
  const throws = (operation, name) => {
    let actual = 'accepted';
    try { operation(); } catch (error) { actual = error.name; }
    equal(actual, name, 'exception name');
  };
  const context = (channels = 2, frames = 256) =>
    new OfflineAudioContext(channels, frames, 8000);
  const bufferSource = (ctx, channels, frames, fill) => {
    const buffer = ctx.createBuffer(channels, frames, ctx.sampleRate);
    for (let channel = 0; channel < channels; channel++)
      fill(buffer.getChannelData(channel), channel);
    return new AudioBufferSourceNode(ctx, { buffer });
  };
  const callbacks = () => {
    const errors = [];
    return {
      guard: operation => event => {
        try { operation(event); } catch (error) { errors.push(error); }
      },
      verify: () => { if (errors.length) throw errors[0]; }
    };
  };
  const run = async () => {
    let passed = 0;
    const failures = [];
    for (const test of cases) {
      try { await test.operation(); passed++; }
      catch (error) { failures.push(test.name + ': ' + error.message); }
    }
    const result = `${passed}/${cases.length} passed`;
    document.getElementById('audio-result').textContent = result;
    document.getElementById('audio-result').setAttribute('data-audio-summary', result);
    document.getElementById('audio-failures').textContent = failures.join('\n');
    for (const failure of failures) {
      const item = document.createElement('p');
      item.className = 'audio-failure';
      item.setAttribute('data-audio-failure', failure);
      document.getElementById('audio-failures').append(item);
    }
    document.getElementById('fixture-status').textContent = 'Fixture ready';
    for (const observation of observations) {
      const item = document.createElement('p');
      item.className = 'audio-observation';
      item.setAttribute('data-audio-observation', JSON.stringify(observation));
      document.getElementById('audio-observations').append(item);
    }
    document.documentElement.dataset.fixtureReady = 'true';
    console.log('Audio routing: ' + result);
  };

  check('generic inherited options', () => {
    const ctx = context();
    const options = {channelCount: 4, channelCountMode: 'explicit',
      channelInterpretation: 'discrete'};
    const nodes = [new GainNode(ctx, options), new BiquadFilterNode(ctx, options),
      new DelayNode(ctx, options), new WaveShaperNode(ctx, options),
      new AnalyserNode(ctx, options), new OscillatorNode(ctx, options),
      new IIRFilterNode(ctx, {...options, feedback: [1], feedforward: [1]})];
    for (const node of nodes) {
      equal(node.channelCount, 4, node.constructor.name + ' count');
      equal(node.channelCountMode, 'explicit', node.constructor.name + ' mode');
      equal(node.channelInterpretation, 'discrete', node.constructor.name + ' interpretation');
    }
  });
  check('standalone source dictionaries ignore unknown members', () => {
    const ctx = context(), options = {};
    for (const name of ['channelCount', 'channelCountMode', 'channelInterpretation'])
      Object.defineProperty(options, name, {get() {throw Error('unknown member read');}});
    for (const Type of [ConstantSourceNode, AudioBufferSourceNode]) {
      const node = new Type(ctx, options);
      equal(node.channelCount, 2, Type.name + ' count');
      node.channelCount = 4;
      equal(node.channelCount, 4, Type.name + ' inherited setter');
    }
  });
  check('restricted stereo options', () => {
    const ctx = context();
    for (const Type of [PannerNode, StereoPannerNode, DynamicsCompressorNode, ConvolverNode]) {
      const node = new Type(ctx);
      equal(node.channelCount, 2, Type.name + ' count');
      equal(node.channelCountMode, 'clamped-max', Type.name + ' mode');
      throws(() => { node.channelCount = 3; }, 'NotSupportedError');
      throws(() => { node.channelCountMode = 'max'; }, 'NotSupportedError');
      throws(() => new Type(ctx, {channelCount: 3}), 'NotSupportedError');
      node.channelCount = 1;
      equal(node.channelCount, 1, Type.name + ' mono');
    }
  });
  check('splitter fixed layout', () => {
    const ctx = context();
    const node = new ChannelSplitterNode(ctx, {numberOfOutputs: 4});
    equal(node.channelCount, 4, 'splitter count');
    equal(node.channelCountMode, 'explicit', 'splitter mode');
    equal(node.channelInterpretation, 'discrete', 'splitter interpretation');
    throws(() => { node.channelCount = 2; }, 'InvalidStateError');
    throws(() => { node.channelCountMode = 'max'; }, 'InvalidStateError');
    throws(() => { node.channelInterpretation = 'speakers'; }, 'InvalidStateError');
  });
  check('merger fixed mono inputs', () => {
    const ctx = context();
    const node = new ChannelMergerNode(ctx, {numberOfInputs: 4});
    equal(node.channelCount, 1, 'merger count');
    equal(node.channelCountMode, 'explicit', 'merger mode');
    throws(() => { node.channelCount = 2; }, 'InvalidStateError');
    throws(() => { node.channelCountMode = 'max'; }, 'InvalidStateError');
    node.channelInterpretation = 'discrete';
    equal(node.channelInterpretation, 'discrete', 'merger interpretation');
  });
  check('offline destination fixed channels', () => {
    const node = context(4).destination;
    equal(node.channelCount, 4, 'destination count');
    throws(() => { node.channelCount = 2; }, 'InvalidStateError');
    throws(() => { node.channelCountMode = 'max'; }, 'InvalidStateError');
    node.channelInterpretation = 'discrete';
    equal(node.channelInterpretation, 'discrete', 'destination interpretation');
  });
  check('dictionary conversion order and getter count', () => {
    const ctx = context(), reads = [];
    const options = {};
    for (const [name, value] of [['channelCount', 2], ['channelCountMode', 'explicit'],
      ['channelInterpretation', 'speakers'], ['gain', 0.5]])
      Object.defineProperty(options, name, {get() {
        reads.push(name);
        return value;
      }});
    const node = new GainNode(ctx, options);
    equal(reads.join(','), 'channelCount,channelCountMode,channelInterpretation,gain',
      'dictionary read order');
    equal(node.gain.value, 0.5, 'converted gain');
  });
  check('unsigned count conversion', () => {
    const ctx = context(), gain = ctx.createGain();
    gain.channelCount = 4294967298;
    equal(gain.channelCount, 2, 'unsigned wrap');
    gain.channelCount = 3.9;
    equal(gain.channelCount, 3, 'unsigned truncation');
    throws(() => { gain.channelCount = 1n; }, 'TypeError');
    throws(() => { gain.channelCount = Symbol(); }, 'TypeError');
    throws(() => { gain.channelCount = 0; }, 'NotSupportedError');
    equal(gain.channelCount, 3, 'failed setter must be atomic');
  });
  check('enum conversion and invalid values', () => {
    const gain = context().createGain();
    gain.channelCountMode = {toString() { return 'explicit'; }};
    equal(gain.channelCountMode, 'explicit', 'enum string conversion');
    gain.channelCountMode = 'other';
    throws(() => new GainNode(gain.context, {channelCountMode: 'other'}), 'TypeError');
    throws(() => { gain.channelInterpretation = Symbol(); }, 'TypeError');
    equal(gain.channelCountMode, 'explicit', 'failed enum setter must be atomic');
  });
  return { check, equal, near, throws, context, bufferSource, observe, callbacks, run };
})();
