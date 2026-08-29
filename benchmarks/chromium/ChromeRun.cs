using System.Diagnostics;
using System.Globalization;
using System.Net.Http.Json;
using System.Text.Json;

namespace ChromiumBaseline;

internal static class ChromeRun
{
    public static async Task<BenchmarkResult> ExecuteAsync(Options options)
    {
        var profile = Path.Combine(Path.GetTempPath(), $"breeze-chromium-{Guid.NewGuid():N}");
        Directory.CreateDirectory(profile);
        Process? chrome = null;
        var stopwatch = Stopwatch.StartNew();
        var timeout = TimeSpan.FromMilliseconds(options.TimeoutMs);
        var result = NewResult(options);
        try
        {
            chrome = StartChrome(options, profile);
            var port = await WaitForDevToolsAsync(chrome, profile, stopwatch, timeout);
            result.WindowReadyMs = stopwatch.Elapsed.TotalMilliseconds;
            var pageSocket = await FindPageSocketAsync(port, timeout);
            using var cdp = new CdpConnection();
            await cdp.ConnectAsync(pageSocket, timeout);
            using var filmstripCdp = options.FilmstripDirectory is null ? null : new CdpConnection();
            if (filmstripCdp is not null)
            {
                await filmstripCdp.ConnectAsync(pageSocket, timeout);
                await filmstripCdp.CallAsync(50_000, "Page.enable", null, timeout);
            }
            var nextId = 1;
            await cdp.CallAsync(nextId++, "Page.enable", null, timeout);
            await cdp.CallAsync(nextId++, "Runtime.enable", null, timeout);
            await cdp.CallAsync(nextId++, "Network.enable", null, timeout);
            await cdp.CallAsync(nextId++, "Performance.enable", new { timeDomain = "timeTicks" }, timeout);
            await cdp.CallAsync(nextId++, "Emulation.setDeviceMetricsOverride", new
            {
                width = options.ViewportWidth,
                height = options.ViewportHeight,
                deviceScaleFactor = options.DeviceScaleFactor,
                mobile = false
            }, timeout);
            await cdp.CallAsync(nextId++, "Emulation.setLocaleOverride", new
            {
                locale = options.Locale.Replace('-', '_')
            }, timeout);
            await cdp.CallAsync(nextId++, "Network.setCacheDisabled", new { cacheDisabled = true }, timeout);
            var version = await cdp.CallAsync(nextId++, "Browser.getVersion", null, timeout);
            result.ChromeVersion = version.GetProperty("product").GetString() ?? string.Empty;

            var navigationStarted = stopwatch.Elapsed;
            const int navigationId = 1000;
            await cdp.SendAsync(new
            {
                id = navigationId,
                method = "Page.navigate",
                @params = new { url = options.Url }
            });
            var filmstripTask = filmstripCdp is null
                ? Task.CompletedTask
                : FilmstripCapture.RunAsync(
                    filmstripCdp,
                    options,
                    stopwatch,
                    navigationStarted,
                    timeout);
            string? navigationError = null;
            await cdp.ReadUntilAsync(root =>
            {
                if (root.TryGetProperty("id", out var id) && id.GetInt32() == navigationId &&
                    root.TryGetProperty("result", out var navigation) &&
                    navigation.TryGetProperty("errorText", out var errorText))
                {
                    navigationError = errorText.GetString();
                }
                if (root.TryGetProperty("method", out var method) &&
                    method.GetString() == "Network.responseReceived" &&
                    root.GetProperty("params").TryGetProperty("type", out var type) &&
                    type.GetString() == "Document")
                {
                    var response = root.GetProperty("params").GetProperty("response");
                    result.HttpStatus = (int)response.GetProperty("status").GetDouble();
                }
                return root.TryGetProperty("method", out method) && method.GetString() == "Page.loadEventFired";
            }, timeout);
            result.PageReadyMs = stopwatch.Elapsed.TotalMilliseconds;
            result.NavigationMs = (stopwatch.Elapsed - navigationStarted).TotalMilliseconds;
            result.Error = navigationError;

            var firstPaint = await EvaluateAsync(cdp, nextId++, BrowserScripts.FirstPaint, timeout);
            result.FirstUsablePaintMs = firstPaint.ValueKind == JsonValueKind.Number
                ? firstPaint.GetDouble()
                : result.NavigationMs;
            var beforeSettle = ProcessTree.Sample(chrome.Id);
            await Task.Delay(options.SettleMs);
            await filmstripTask;
            var afterSettle = ProcessTree.Sample(chrome.Id);
            result.AverageCpuPercent = CpuPercent(beforeSettle, afterSettle, options.SettleMs);

            var probe = await EvaluateAsync(cdp, nextId++, BrowserScripts.DocumentProbe, timeout);
            result.FinalUrl = probe.GetProperty("url").GetString() ?? options.Url;
            result.BodyTextLength = probe.GetProperty("bodyTextLength").GetInt32();
            result.ElementCount = probe.GetProperty("elementCount").GetInt32();
            result.BrowserErrorSurface = probe.GetProperty("browserErrorSurface").GetBoolean();
            result.DocumentHeightCssPx = probe.GetProperty("documentHeight").GetInt32();
            result.FixtureReady = probe.GetProperty("fixtureReady").GetBoolean();
            var innerWidth = probe.GetProperty("innerWidth").GetInt32();
            var innerHeight = probe.GetProperty("innerHeight").GetInt32();
            result.ViewportWidthCssPx = innerWidth;
            result.ViewportHeightCssPx = innerHeight;
            // Chromium quantizes device pixels before deriving the CSS viewport. At
            // fractional scale factors this can move either dimension by one CSS px.
            if (Math.Abs(innerWidth - options.ViewportWidth) > 1 ||
                Math.Abs(innerHeight - options.ViewportHeight) > 1)
            {
                result.Error ??= $"Chromium viewport was {innerWidth}x{innerHeight}, expected {options.ViewportWidth}x{options.ViewportHeight}.";
            }
            if (options.RequireFixtureReady && !result.FixtureReady)
            {
                result.Error ??= "Fixture readiness marker was not observed.";
            }

            var compatibilityStarted = stopwatch.Elapsed;
            var compatibility = await CompatibilityCapture.CollectAsync(cdp, nextId, timeout);
            nextId = compatibility.NextCommandId;
            compatibility.ApplyTo(result);
            result.CompatibilityCaptureMs = (stopwatch.Elapsed - compatibilityStarted).TotalMilliseconds;
            if (!string.IsNullOrWhiteSpace(options.Screenshot))
            {
                var screenshot = Path.GetFullPath(options.Screenshot);
                Directory.CreateDirectory(Path.GetDirectoryName(screenshot)!);
                await File.WriteAllBytesAsync(screenshot, compatibility.ScreenshotPng);
                result.Screenshot = screenshot;
            }
            result.Error ??= PageClassification.Classify(result);

            if (options.ScrollSamples > 0)
            {
                result.SteadyScroll = ParseScroll(await EvaluateAsync(
                    cdp,
                    nextId++,
                    BrowserScripts.SteadyScroll(options.ScrollSamples),
                    timeout));
            }
            if (options.EarlyScroll)
            {
                result.EarlyScroll = ParseScroll(await EvaluateAsync(
                    cdp,
                    nextId++,
                    BrowserScripts.EarlyScroll,
                    timeout + TimeSpan.FromSeconds(7)));
            }
            await EvaluateAsync(cdp, nextId++, "window.scrollTo(0, 0); 0", timeout);

            var metrics = await cdp.CallAsync(nextId++, "Performance.getMetrics", null, timeout);
            result.JavascriptMs = Metric(metrics, "ScriptDuration") * 1_000;
            result.StyleRefreshMs = Metric(metrics, "RecalcStyleDuration") * 1_000;
            result.LayoutMs = Metric(metrics, "LayoutDuration") * 1_000;
            result.TaskMs = Metric(metrics, "TaskDuration") * 1_000;

            var finalSample = ProcessTree.Sample(chrome.Id);
            result.WorkingSetBytes = finalSample.WorkingSetBytes;
            result.PrivateBytes = finalSample.PrivateBytes;
            result.PeakWorkingSetBytes = finalSample.PeakWorkingSetBytes;
            result.CpuTimeMs = finalSample.CpuTimeMs;
            result.ProcessCount = finalSample.ProcessCount;
            return result;
        }
        catch (Exception exception)
        {
            result.Error = exception.Message;
            return result;
        }
        finally
        {
            if (chrome is not null)
            {
                if (!chrome.HasExited)
                {
                    chrome.Kill(entireProcessTree: true);
                    await chrome.WaitForExitAsync();
                }
                chrome.Dispose();
            }
            DeleteFreshProfile(profile);
        }
    }

    private static BenchmarkResult NewResult(Options options) => new()
    {
        RequestedUrl = options.Url,
        RequestedViewportWidthCssPx = options.ViewportWidth,
        RequestedViewportHeightCssPx = options.ViewportHeight,
        ViewportWidthCssPx = options.ViewportWidth,
        ViewportHeightCssPx = options.ViewportHeight,
        DeviceScaleFactor = options.DeviceScaleFactor,
        Locale = options.Locale,
        SettleMs = options.SettleMs
    };

    private static Process StartChrome(Options options, string profile)
    {
        var start = new ProcessStartInfo
        {
            FileName = options.ChromePath,
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden
        };
        foreach (var argument in new[]
        {
            "--headless",
            $"--user-data-dir={profile}",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-extensions",
            "--disable-sync",
            $"--lang={options.Locale}",
            $"--force-device-scale-factor={options.DeviceScaleFactor.ToString(CultureInfo.InvariantCulture)}",
            $"--window-size={options.ViewportWidth},{options.ViewportHeight}",
            "about:blank"
        })
        {
            start.ArgumentList.Add(argument);
        }
        return Process.Start(start) ?? throw new InvalidOperationException("Chromium did not start.");
    }

    private static async Task<int> WaitForDevToolsAsync(Process chrome, string profile, Stopwatch stopwatch, TimeSpan timeout)
    {
        var activePort = Path.Combine(profile, "DevToolsActivePort");
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            if (chrome.HasExited)
            {
                throw new InvalidOperationException($"Chromium exited with code {chrome.ExitCode}.");
            }
            if (ProcessTree.HasVisibleWindow(chrome.Id))
            {
                throw new InvalidOperationException("Headless Chromium exposed a visible window.");
            }
            if (File.Exists(activePort))
            {
                try
                {
                    var lines = await File.ReadAllLinesAsync(activePort);
                    if (lines.Length >= 2 && int.TryParse(lines[0], out var port))
                    {
                        return port;
                    }
                }
                catch (IOException)
                {
                    // Chromium is still publishing the file atomically.
                }
            }
            await Task.Delay(20);
        }
        throw new TimeoutException($"Timed out after {stopwatch.Elapsed.TotalSeconds:F1}s waiting for Chromium DevTools.");
    }

    private static async Task<Uri> FindPageSocketAsync(int port, TimeSpan timeout)
    {
        using var client = new HttpClient { Timeout = timeout };
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            try
            {
                var targets = await client.GetFromJsonAsync<JsonElement>($"http://127.0.0.1:{port}/json/list");
                foreach (var target in targets.EnumerateArray())
                {
                    if (target.GetProperty("type").GetString() == "page")
                    {
                        return new Uri(target.GetProperty("webSocketDebuggerUrl").GetString()!);
                    }
                }
            }
            catch (HttpRequestException)
            {
                // DevTools is not accepting HTTP requests yet.
            }
            await Task.Delay(20);
        }
        throw new TimeoutException("Timed out waiting for a Chromium page target.");
    }

    private static async Task<JsonElement> EvaluateAsync(CdpConnection cdp, int id, string expression, TimeSpan timeout)
    {
        var response = await cdp.CallAsync(id, "Runtime.evaluate", new
        {
            expression,
            returnByValue = true,
            awaitPromise = true
        }, timeout);
        if (response.TryGetProperty("exceptionDetails", out var exception))
        {
            throw new InvalidOperationException($"Chromium evaluation failed: {exception}");
        }
        var remote = response.GetProperty("result");
        return remote.TryGetProperty("value", out var value) ? value.Clone() : default;
    }

    private static ScrollMetrics ParseScroll(JsonElement value) => new()
    {
        Samples = value.GetProperty("samples").GetInt32(),
        AverageMs = value.GetProperty("averageMs").GetDouble(),
        P95Ms = value.GetProperty("p95Ms").GetDouble(),
        MaximumMs = value.GetProperty("maximumMs").GetDouble(),
        LongFrames = value.GetProperty("longFrames").GetInt32(),
        FinalScrollY = (int)Math.Round(value.GetProperty("finalScrollY").GetDouble())
    };

    private static double Metric(JsonElement result, string name)
    {
        foreach (var metric in result.GetProperty("metrics").EnumerateArray())
        {
            if (metric.GetProperty("name").GetString() == name)
            {
                return metric.GetProperty("value").GetDouble();
            }
        }
        return 0;
    }

    private static double CpuPercent(ProcessSample before, ProcessSample after, int elapsedMs)
    {
        var cpuMs = Math.Max(0, after.CpuTimeMs - before.CpuTimeMs);
        return cpuMs / Math.Max(elapsedMs, 1) / Math.Max(Environment.ProcessorCount, 1) * 100;
    }

    private static void DeleteFreshProfile(string profile)
    {
        var full = Path.GetFullPath(profile);
        var expectedPrefix = Path.Combine(Path.GetTempPath(), "breeze-chromium-");
        if (!full.StartsWith(expectedPrefix, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Refusing to remove an unexpected Chromium profile path.");
        }
        for (var attempt = 0; attempt < 5; attempt++)
        {
            try
            {
                if (Directory.Exists(full)) Directory.Delete(full, recursive: true);
                return;
            }
            catch (Exception error) when (attempt < 4 && error is IOException or UnauthorizedAccessException)
            {
                Thread.Sleep(100);
            }
        }
    }

}
