(() => {
  const {check, equal, throws, context} = AudioRoutingChecks;
  check('AudioBuffer lexical dictionary conversion', () => {
    const reads = [], options = {};
    for (const [name, value] of [['length', 8.9], ['numberOfChannels', 4294967298], ['sampleRate', 8000]])
      Object.defineProperty(options, name, {get() {reads.push(name); return value;}});
    const buffer = new AudioBuffer(options);
    equal(reads.join(','), 'length,numberOfChannels,sampleRate', 'dictionary order');
    equal(buffer.length, 8, 'unsigned length');
    equal(buffer.numberOfChannels, 2, 'unsigned channel wrap');
  });
  check('AudioBuffer required members and nominal ranges', () => {
    throws(() => new AudioBuffer({sampleRate: 8000}), 'TypeError');
    throws(() => new AudioBuffer({length: 8}), 'TypeError');
    throws(() => new AudioBuffer({length: 0, sampleRate: 8000}), 'NotSupportedError');
    throws(() => new AudioBuffer({length: 8, sampleRate: -1}), 'NotSupportedError');
    throws(() => new AudioBuffer({length: 8, sampleRate: 8000, numberOfChannels: 1n}), 'TypeError');
  });
  check('createBuffer converts all arguments before validation', () => {
    const ctx = context(), reads = [];
    const value = (name, number) => ({valueOf() {reads.push(name); return number;}});
    throws(() => ctx.createBuffer(value('channels', 0), value('length', 8), value('rate', 8000)),
      'NotSupportedError');
    equal(reads.join(','), 'channels,length,rate', 'factory conversion order');
    throws(() => ctx.createBuffer(1, 8), 'TypeError');
    throws(() => ctx.createBuffer(1n, 8, 8000), 'TypeError');
  });
  check('AudioBuffer methods require arguments and reject forged receivers', () => {
    const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
    throws(() => buffer.getChannelData(), 'TypeError');
    throws(() => buffer.copyToChannel(new Float32Array(1)), 'TypeError');
    throws(() => AudioBuffer.prototype.getChannelData.call({}, 0), 'TypeError');
    throws(() => buffer.getChannelData(1n), 'TypeError');
    equal(buffer.getChannelData(4294967296), buffer.getChannelData(0), 'unsigned index');
  });
  check('channel copying uses private storage rather than public overrides', () => {
    const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
    const channel = buffer.getChannelData(0);
    buffer.getChannelData = () => {throw Error('overridden method used');};
    Object.defineProperty(buffer, 'length', {get() {throw Error('overridden length used');}});
    buffer.copyToChannel(new Float32Array([0.25]), 0, 2);
    const result = new Float32Array(1);
    buffer.copyFromChannel(result, 0, 2);
    equal(channel[2], 0.25, 'private destination');
    equal(result[0], 0.25, 'private source');
  });
  check('channel copy conversion precedes range validation', () => {
    const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
    const array = new Float32Array(1);
    for (const method of ['copyFromChannel', 'copyToChannel']) {
      throws(() => buffer[method](array, 99, 1n), 'TypeError');
      throws(() => buffer[method](array, 99, 0), 'IndexSizeError');
    }
  });
  check('channel copies preserve unused destination elements and allow overlap', () => {
    const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
    const channel = buffer.getChannelData(0);
    channel.set([1,2,3,4,5,6,7,8]);
    const result = new Float32Array([9,9,9,9]);
    buffer.copyFromChannel(result, 0, 6);
    equal(result.join(','), '7,8,9,9', 'unwritten array tail');
    buffer.copyToChannel(channel.subarray(0, 6), 0, 2);
    equal(channel.join(','), '1,2,1,2,3,4,5,6', 'overlapping copy');
  });
  check('channel copy accepts detached empty views but rejects resizable storage', () => {
    const buffer = new AudioBuffer({length: 8, sampleRate: 8000});
    const detached = new Float32Array(4);
    structuredClone(detached.buffer, {transfer: [detached.buffer]});
    buffer.copyFromChannel(detached, 0);
    buffer.copyToChannel(detached, 0);
    const resizable = new ArrayBuffer(16, {maxByteLength: 32});
    const view = new Float32Array(resizable);
    throws(() => buffer.copyFromChannel(view, 0), 'TypeError');
    throws(() => buffer.copyToChannel(view, 0), 'TypeError');
    equal(buffer.getChannelData(0).every(value => value === 0), true, 'empty copies preserve PCM');
  });
})();
