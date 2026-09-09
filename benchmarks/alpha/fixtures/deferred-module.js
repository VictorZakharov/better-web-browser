import './deferred-dependency.js?delay_ms=3000';
record('module');
await new Promise(() => {});
record('must-not-complete');
