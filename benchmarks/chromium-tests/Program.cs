using ChromiumBaseline;

internal static class Program
{
    private static async Task<int> Main()
    {
        var chrome = Options.FindChrome();
        var root = Path.Combine(Path.GetTempPath(), $"breeze-chromium-tests-{Guid.NewGuid():N}");
        Directory.CreateDirectory(root);
        try
        {
            var light = await CaptureAsync(chrome, root, "light", """
                <!doctype html><html><body>
                  <main><h1>Light DOM control</h1><p>This ordinary document remains a valid populated baseline.</p></main>
                </body></html>
                """);
            Assert(light.Error is null, $"light DOM control failed: {light.Error}");
            Assert(light.BodyTextLength >= 20, "light DOM control did not expose light-DOM text");

            var open = await CaptureAsync(chrome, root, "open-shadow", ShadowFixture("open"));
            Assert(open.Error is null, $"open Shadow DOM page failed: {open.Error}");
            Assert(open.BodyTextLength < 20, "open Shadow DOM fixture unexpectedly relied on light-DOM text");
            Assert(open.ShadowRootCount == 1,
                $"open shadow root count was {open.ShadowRootCount}, expected one flattened root");
            Assert(open.ComposedTextLength >= 20, "open shadow content was absent from layout text");
            Assert(open.PaintedPixelRatio >= 0.001, "open shadow content was absent from the screenshot signal");

            var closed = await CaptureAsync(chrome, root, "closed-shadow", ShadowFixture("closed"));
            Assert(closed.Error is null, $"closed Shadow DOM page failed: {closed.Error}");
            Assert(closed.BodyTextLength < 20, "closed Shadow DOM fixture unexpectedly relied on light-DOM text");
            Assert(closed.ShadowRootCount == 1,
                $"closed shadow root count was {closed.ShadowRootCount}, expected one flattened root");
            Assert(closed.ComposedTextLength >= 20, "closed shadow content was absent from layout text");
            Assert(closed.PaintedPixelRatio >= 0.001, "closed shadow content was absent from the screenshot signal");

            var blank = await CaptureAsync(chrome, root, "blank", "<!doctype html><html><body></body></html>");
            Assert(blank.Error == PageClassification.EmptyDocumentError,
                $"blank control was not rejected: {blank.Error ?? "no error"}");
            Assert(blank.PaintedPixelRatio < 0.001, "blank control produced a nonblank screenshot signal");

            var browserError = await CaptureUrlAsync(chrome, root, "browser-error", "http://127.0.0.1:1/");
            Assert(browserError.Error is not null, "browser error surface was accepted as page content");
            Assert(browserError.BrowserErrorSurface, "Chromium error surface was not identified");
            Assert(browserError.PaintedPixelRatio >= 0.001,
                "browser-error control did not exercise the painted-error rejection path");
            Assert(PageClassification.Classify(browserError) == PageClassification.BrowserError,
                "painted browser error surface was accepted by the page classifier");
            Console.WriteLine("Chromium compatibility capture self-tests passed.");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error.Message);
            return 1;
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }

    private static async Task<BenchmarkResult> CaptureAsync(
        string chrome,
        string root,
        string name,
        string html)
    {
        return await CaptureUrlAsync(
            chrome,
            root,
            name,
            "data:text/html;charset=utf-8," + Uri.EscapeDataString(html));
    }

    private static async Task<BenchmarkResult> CaptureUrlAsync(
        string chrome,
        string root,
        string name,
        string url)
    {
        var options = new Options
        {
            Url = url,
            Output = Path.Combine(root, name + ".json"),
            ChromePath = chrome,
            ViewportWidth = 640,
            ViewportHeight = 400,
            SettleMs = 100,
            TimeoutMs = 15_000
        };
        return await ChromeRun.ExecuteAsync(options);
    }

    private static string ShadowFixture(string mode) => $$"""
        <!doctype html><html><body><article-card></article-card><script>
          document.querySelector('article-card').attachShadow({mode: '{{mode}}'}).innerHTML =
            '<style>article{font:24px sans-serif;color:#123b7a}</style>' +
            '<article>Visible {{mode}} shadow content proves the composed page is populated.</article>';
        </script></body></html>
        """;

    private static void Assert(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
