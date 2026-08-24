(() => {
  const markReady = () => {
    document.documentElement.dataset.fixtureReady = 'true';
    const status = document.getElementById('fixture-status');
    if (status) status.textContent = 'Fixture ready';
  };
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', markReady, { once: true });
  } else {
    markReady();
  }
})();
