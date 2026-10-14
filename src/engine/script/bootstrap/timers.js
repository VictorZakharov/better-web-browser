    const timers = new Map();
    const describeTimerCallback = callback => {
        if (typeof callback !== 'function') return 'string callback';
        const name = String(callback.name || '').trim();
        try {
            const source = Function.prototype.toString.call(callback).replace(/\s+/g, ' ').trim();
            return (name ? name + ': ' + source : source || 'anonymous callback').slice(0, 160);
        }
        catch (_) { return (name || 'anonymous callback').slice(0, 160); }
    };
    const queueTimer = (callback, delay, repeat, args, label = describeTimerCallback(callback),
        scheduleOperation = 'timerSchedule') => {
        const id = nextTimer++;
        delay = Math.max(0, Number(delay) || 0);
        timers.set(id, { callback, repeat, args, label, cancelable: scheduleOperation === 'timerSchedule' });
        host(scheduleOperation, id, delay, repeat);
        return id;
    };
    const queueMediaTask = callback => queueTimer(callback, 0, false, [], 'media element task', 'mediaTaskSchedule');
    windowObject.__fontLoadingQueue = callback =>
        queueTimer(callback, 0, false, [], 'font loading task', 'fontTaskSchedule');
    const queueWebGlContextTask = callback =>
        queueTimer(callback, 0, false, [], 'WebGL context lifecycle', 'mediaTaskSchedule');
    // Hand off private scheduling and host calls to the Web Audio bootstrap.
    // web_audio_base.js consumes and deletes both hooks before author scripts run.
    windowObject.__webAudioRenderTask = (callback, delay = 0) =>
        queueTimer(callback, delay, false, [], 'Web Audio render task',
            delay > 0 ? 'audioTaskSchedule' : 'mediaTaskSchedule');
    windowObject.__webAudioHostCall = (...args) => host(...args);
    windowObject.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    windowObject.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    windowObject.clearTimeout = windowObject.clearInterval = id => {
        id = Number(id);
        if (timers.get(id)?.cancelable === false) return;
        timers.delete(id);
        host('timerCancel', id);
    };
