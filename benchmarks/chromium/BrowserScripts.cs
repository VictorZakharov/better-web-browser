namespace ChromiumBaseline;

internal static class BrowserScripts
{
    public const string DocumentProbe = """
        (() => ({
          url: location.href,
          bodyTextLength: (document.body?.innerText || '').trim().length,
          elementCount: document.querySelectorAll('*').length,
          browserErrorSurface: location.protocol === 'chrome-error:' ||
            document.documentURI.startsWith('chrome-error://'),
          documentHeight: Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight || 0),
          fixtureReady: document.documentElement.dataset.fixtureReady === 'true',
          innerWidth: window.innerWidth,
          innerHeight: window.innerHeight
        }))()
        """;

    public const string FirstPaint = """
        (() => {
          const paint = performance.getEntriesByName('first-contentful-paint')[0];
          return paint ? paint.startTime : performance.getEntriesByType('navigation')[0]?.domContentLoadedEventEnd || 0;
        })()
        """;

    public static string SteadyScroll(int samples) => $$"""
        (async () => {
          const count = {{samples}};
          const values = [];
          const maximum = Math.max(0, document.documentElement.scrollHeight - innerHeight);
          for (let index = 1; index <= count; index++) {
            const started = performance.now();
            scrollTo(0, Math.round(maximum * index / (count + 1)));
            await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            values.push(performance.now() - started);
          }
          values.sort((left, right) => left - right);
          const total = values.reduce((sum, value) => sum + value, 0);
          return { samples: values.length, averageMs: total / Math.max(values.length, 1),
            p95Ms: values[Math.max(0, Math.ceil(values.length * .95) - 1)] || 0,
            maximumMs: values[values.length - 1] || 0,
            longFrames: values.filter(value => value > 50).length, finalScrollY: scrollY };
        })()
        """;

    public const string EarlyScroll = """
        (async () => {
          const values = [];
          const maximum = Math.max(0, document.documentElement.scrollHeight - innerHeight);
          let direction = 1;
          for (let index = 0; index < 375; index++) {
            const deadline = performance.now() + 16;
            let target = scrollY + 42 * direction;
            if (target >= maximum) { target = maximum; direction = -1; }
            if (target <= 0) { target = 0; direction = 1; }
            const started = performance.now();
            scrollTo(0, target);
            await new Promise(resolve => requestAnimationFrame(resolve));
            values.push(performance.now() - started);
            const delay = deadline - performance.now();
            if (delay > 0) await new Promise(resolve => setTimeout(resolve, delay));
          }
          values.sort((left, right) => left - right);
          const total = values.reduce((sum, value) => sum + value, 0);
          return { samples: values.length, averageMs: total / values.length,
            p95Ms: values[Math.max(0, Math.ceil(values.length * .95) - 1)] || 0,
            maximumMs: values[values.length - 1] || 0,
            longFrames: values.filter(value => value > 50).length, finalScrollY: scrollY };
        })()
        """;
}
