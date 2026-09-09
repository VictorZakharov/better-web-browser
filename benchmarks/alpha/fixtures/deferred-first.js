record('first-' + document.readyState);
Promise.resolve().then(() => record('first-micro'));
