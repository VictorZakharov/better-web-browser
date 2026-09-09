record('module');
await new Promise(resolve => setTimeout(resolve, 3500));
record('awaited');
