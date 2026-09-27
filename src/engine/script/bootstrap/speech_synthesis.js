// Web Speech synthesis runs in the browser process. The renderer owns only queue and DOM events.
// https://wicg.github.io/speech-api/#tts-section
(() => {
    'use strict';
    const native = __hostCall;
    const trusted = globalThis.__markTrustedEvent || (event => event);
    const pending = new Map();
    let voices = [];
    let voiceRequest = 0;
    let voicesInitialized = false;
    const internalConstructor = Symbol('browser-owned speech object');

    const dispatch = (target, event) => {
        target.dispatchEvent(trusted(event));
        const handler = target['on' + event.type];
        if (typeof handler === 'function') {
            try { handler.call(target, event); }
            catch (error) { queueMicrotask(() => { throw error; }); }
        }
    };
    const send = command => Number(native('speechRequest', JSON.stringify(command)));
    const numeric = (value, fallback) => {
        const number = Number(value);
        return Number.isFinite(number) ? number : fallback;
    };

    class SpeechSynthesisVoice {
        constructor({ voiceURI, name, lang, default: isDefault }, key) {
            if (key !== internalConstructor) throw new TypeError('Illegal constructor');
            this.voiceURI = String(voiceURI);
            this.name = String(name);
            this.lang = String(lang);
            this.default = Boolean(isDefault);
            this.localService = true;
            Object.freeze(this);
        }
    }

    class SpeechSynthesisEvent extends Event {
        constructor(type, init) {
            if (!(init?.utterance instanceof SpeechSynthesisUtterance))
                throw new TypeError('SpeechSynthesisEvent requires an utterance');
            super(type, init);
            this.utterance = init.utterance;
            this.charIndex = Number(init.charIndex) || 0;
            this.charLength = Number(init.charLength) || 0;
            this.elapsedTime = Number(init.elapsedTime) || 0;
            this.name = String(init.name || '');
        }
    }

    class SpeechSynthesisErrorEvent extends SpeechSynthesisEvent {
        constructor(type, init) {
            if (!init || !('error' in init))
                throw new TypeError('SpeechSynthesisErrorEvent requires an error');
            super(type, init);
            this.error = String(init.error);
        }
    }

    class SpeechSynthesisUtterance extends EventTarget {
        constructor(text = '') {
            super();
            this.text = String(text);
            this.lang = '';
            this.voice = null;
            this.volume = 1;
            this.rate = 1;
            this.pitch = 1;
            this.onstart = null;
            this.onend = null;
            this.onerror = null;
            this.onpause = null;
            this.onresume = null;
            this.onmark = null;
            this.onboundary = null;
        }
    }

    class SpeechSynthesis extends EventTarget {
        constructor(key) {
            if (key !== internalConstructor) throw new TypeError('Illegal constructor');
            super();
            this._queue = [];
            this._current = null;
            this._started = false;
            this._paused = false;
            this.onvoiceschanged = null;
        }
        get pending() { return this._queue.length !== 0 || (!!this._current && !this._started); }
        get speaking() { return !!this._current && this._started; }
        get paused() { return this._paused; }
        getVoices() {
            if (!voicesInitialized && !voiceRequest) requestVoices();
            return voices.slice();
        }
        speak(utterance) {
            if (!(utterance instanceof SpeechSynthesisUtterance))
                throw new TypeError('SpeechSynthesis.speak requires a SpeechSynthesisUtterance');
            if (utterance.voice !== null && !(utterance.voice instanceof SpeechSynthesisVoice))
                throw new TypeError('SpeechSynthesisUtterance.voice must be a SpeechSynthesisVoice');
            const text = String(utterance.text);
            if (new TextEncoder().encode(text).length > 16 * 1024)
                throw new DOMException('Utterance exceeds the browser limit', 'QuotaExceededError');
            // The renderer and protocol validate the numeric range before any audio starts.
            for (const [name, lower, upper] of [
                ['rate', 0.1, 10], ['pitch', 0, 2], ['volume', 0, 1]
            ]) {
                const value = Number(utterance[name]);
                if (!Number.isFinite(value) || value < lower || value > upper)
                    throw new RangeError(`SpeechSynthesisUtterance.${name} is out of range`);
            }
            // Submit immediately while the trusted activation is still live. The native
            // worker serializes audio; delaying IPC until a prior utterance ends would
            // require a second activation long after the initiating click.
            const command = {
                kind: 'speak', text,
                voiceUri: utterance.voice?.voiceURI || '',
                lang: String(utterance.lang || ''),
                rate: numeric(utterance.rate, 1),
                pitch: numeric(utterance.pitch, 1),
                volume: numeric(utterance.volume, 1),
            };
            const id = send(command);
            const entry = { id, utterance };
            pending.set(id, entry);
            if (this._current) this._queue.push(entry);
            else { this._current = entry; this._started = false; }
        }
        cancel() {
            if (pending.size) send({ kind: 'cancel' });
            this._queue.length = 0;
        }
        pause() {
            if (this._paused) return;
            this._paused = true;
            send({ kind: 'pause' });
        }
        resume() {
            if (!this._paused) return;
            this._paused = false;
            send({ kind: 'resume' });
        }
        _receive(id, event) {
            const entry = pending.get(id);
            if (!entry) return;
            const { utterance } = entry;
            if (event.kind === 'start') {
                if (this._current?.id === id) this._started = true;
                dispatch(utterance, new SpeechSynthesisEvent('start', { utterance }));
            } else if (event.kind === 'pause')
                dispatch(utterance, new SpeechSynthesisEvent('pause', { utterance }));
            else if (event.kind === 'resume')
                dispatch(utterance, new SpeechSynthesisEvent('resume', { utterance }));
            else if (event.kind === 'end' || event.kind === 'cancel' || event.kind === 'error') {
                pending.delete(id);
                if (this._current?.id === id) {
                    this._current = this._queue.shift() || null;
                    this._started = false;
                }
                else this._queue = this._queue.filter(queued => queued.id !== id);
                if (event.kind === 'end')
                    dispatch(utterance, new SpeechSynthesisEvent('end', { utterance }));
                else
                    dispatch(utterance, new SpeechSynthesisErrorEvent('error', {
                        utterance,
                        error: event.kind === 'cancel' ? 'canceled' :
                            event.message === 'not-allowed' ? 'not-allowed' : 'synthesis-failed'
                    }));
            }
        }
    }

    const synthesis = new SpeechSynthesis(internalConstructor);
    function requestVoices() {
        if (voiceRequest) return;
        try { voiceRequest = send({ kind: 'getVoices' }); }
        catch (_) { voiceRequest = 0; }
    }
    globalThis.__receiveSpeechSynthesisUpdate = payload => {
        const update = JSON.parse(String(payload));
        const id = Number(update.id);
        const event = update.event || {};
        if (event.kind === 'voices' && id === voiceRequest) {
            const previous = voices;
            voices = (event.voices || []).map(info => new SpeechSynthesisVoice(info, internalConstructor));
            voicesInitialized = true;
            voiceRequest = 0;
            if (previous.length !== voices.length || voices.some((voice, index) =>
                voice.voiceURI !== previous[index]?.voiceURI))
                dispatch(synthesis, new Event('voiceschanged'));
        } else if (event.kind === 'error' && id === voiceRequest) {
            voiceRequest = 0;
        } else {
            synthesis._receive(id, event);
        }
    };
    globalThis.SpeechSynthesis = SpeechSynthesis;
    globalThis.SpeechSynthesisUtterance = SpeechSynthesisUtterance;
    globalThis.SpeechSynthesisVoice = SpeechSynthesisVoice;
    globalThis.SpeechSynthesisEvent = SpeechSynthesisEvent;
    globalThis.SpeechSynthesisErrorEvent = SpeechSynthesisErrorEvent;
    Object.defineProperty(globalThis, 'speechSynthesis', {
        value: synthesis, writable: false, configurable: true, enumerable: true
    });
})();
