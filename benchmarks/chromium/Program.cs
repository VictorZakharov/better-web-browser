using System.Text.Json;

namespace ChromiumBaseline;

internal static class Program
{
    private static async Task<int> Main(string[] arguments)
    {
        try
        {
            var options = Options.Parse(arguments);
            var result = await ChromeRun.ExecuteAsync(options);
            var output = Path.GetFullPath(options.Output);
            Directory.CreateDirectory(Path.GetDirectoryName(output)!);
            await File.WriteAllTextAsync(output, JsonSerializer.Serialize(result, JsonDefaults.Options));
            Console.WriteLine($"Chromium benchmark written to {output}");
            return result.Error is null ? 0 : 2;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception.Message);
            return 1;
        }
    }
}

internal sealed class Options
{
    public required string Url { get; init; }
    public required string Output { get; init; }
    public required string ChromePath { get; init; }
    public string? Screenshot { get; init; }
    public string? FilmstripDirectory { get; init; }
    public int FilmstripIntervalMs { get; init; } = 500;
    public int FilmstripDurationMs { get; init; } = 10_000;
    public int ViewportWidth { get; init; } = 1088;
    public int ViewportHeight { get; init; } = 607;
    public double DeviceScaleFactor { get; init; } = 1;
    public string Locale { get; init; } = "en-US";
    public string? UserAgent { get; init; }
    public int SettleMs { get; init; } = 2_000;
    public int TimeoutMs { get; init; } = 30_000;
    public int ScrollSamples { get; init; }
    public bool EarlyScroll { get; init; }
    public bool RequireFixtureReady { get; init; }
    public ClickPoint? ClickAfterReady { get; init; }
    public string? ActivateLinkAfterReady { get; init; }
    public int NavigationDelayMs { get; init; }
    public IReadOnlyList<string> DiagnosticSelectors { get; init; } = Array.Empty<string>();

    public static Options Parse(string[] arguments)
    {
        var values = new Dictionary<string, string>(StringComparer.Ordinal);
        var switches = new HashSet<string>(StringComparer.Ordinal);
        var diagnosticSelectors = new List<string>();
        for (var index = 0; index < arguments.Length; index++)
        {
            var argument = arguments[index];
            if (argument is "--early-scroll" or "--require-fixture-ready")
            {
                switches.Add(argument);
                continue;
            }
            if (argument == "--diagnostic-selector")
            {
                if (++index >= arguments.Length || string.IsNullOrWhiteSpace(arguments[index]))
                {
                    throw new ArgumentException("--diagnostic-selector requires a non-empty selector.");
                }
                diagnosticSelectors.Add(arguments[index]);
                continue;
            }
            if (!argument.StartsWith("--", StringComparison.Ordinal) || ++index >= arguments.Length)
            {
                throw new ArgumentException($"{argument} requires a value.");
            }
            values[argument] = arguments[index];
        }

        string Required(string name) => values.TryGetValue(name, out var value)
            ? value
            : throw new ArgumentException($"Provide {name}.");
        int Integer(string name, int fallback, int minimum, int maximum) => values.TryGetValue(name, out var value)
            ? Math.Clamp(int.Parse(value), minimum, maximum)
            : fallback;

        var url = Required("--url");
        if (!Uri.TryCreate(url, UriKind.Absolute, out _))
        {
            throw new ArgumentException("--url must be absolute.");
        }
        var chrome = values.GetValueOrDefault("--chrome") ?? FindChrome();
        if (!File.Exists(chrome))
        {
            throw new FileNotFoundException("Chromium executable was not found.", chrome);
        }

        var filmstripDirectory = values.GetValueOrDefault("--filmstrip-directory");
        var filmstripIntervalMs = Integer("--filmstrip-interval-ms", 500, 100, 10_000);
        var filmstripDurationMs = Integer("--filmstrip-duration-ms", 10_000, 100, 60_000);
        if (filmstripDirectory is null &&
            (values.ContainsKey("--filmstrip-interval-ms") || values.ContainsKey("--filmstrip-duration-ms")))
        {
            throw new ArgumentException("Filmstrip timing requires --filmstrip-directory.");
        }
        if (filmstripDirectory is not null &&
            (filmstripDurationMs % filmstripIntervalMs != 0 || filmstripDurationMs / filmstripIntervalMs > 120))
        {
            throw new ArgumentException("Filmstrip duration must be a multiple of its interval and contain at most 120 frames.");
        }
        var clickAfterReady = values.TryGetValue("--click-after-ready", out var clickValue)
            ? ClickPoint.Parse(clickValue)
            : null;

        return new Options
        {
            Url = url,
            Output = Required("--output"),
            ChromePath = Path.GetFullPath(chrome),
            Screenshot = values.GetValueOrDefault("--screenshot"),
            FilmstripDirectory = filmstripDirectory is null ? null : Path.GetFullPath(filmstripDirectory),
            FilmstripIntervalMs = filmstripIntervalMs,
            FilmstripDurationMs = filmstripDurationMs,
            ViewportWidth = Integer("--viewport-width", 1088, 320, 7680),
            ViewportHeight = Integer("--viewport-height", 607, 240, 4320),
            DeviceScaleFactor = values.TryGetValue("--device-scale-factor", out var scale)
                ? Math.Clamp(double.Parse(scale, System.Globalization.CultureInfo.InvariantCulture), 0.5, 4)
                : 1,
            Locale = values.GetValueOrDefault("--locale") ?? "en-US",
            UserAgent = values.GetValueOrDefault("--user-agent"),
            SettleMs = Integer("--settle-ms", 2_000, 100, 60_000),
            TimeoutMs = Integer("--timeout-ms", 30_000, 1_000, 120_000),
            ScrollSamples = Integer("--scroll-samples", 0, 0, 120),
            EarlyScroll = switches.Contains("--early-scroll"),
            RequireFixtureReady = switches.Contains("--require-fixture-ready"),
            ClickAfterReady = clickAfterReady,
            ActivateLinkAfterReady = values.GetValueOrDefault("--activate-link-after-ready"),
            NavigationDelayMs = Integer("--navigation-delay-ms", 0, 0, 60_000)
            ,
            DiagnosticSelectors = diagnosticSelectors
        };
    }

    internal static string FindChrome()
    {
        var programFiles = new[]
        {
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles)
        };
        var candidates = programFiles.SelectMany(root => new[]
        {
            Path.Combine(root, "Google", "Chrome", "Application", "chrome.exe"),
            Path.Combine(root, "Microsoft", "Edge", "Application", "msedge.exe")
        });
        return candidates.FirstOrDefault(File.Exists)
            ?? throw new FileNotFoundException("Install Chrome/Edge or pass --chrome.");
    }
}

internal sealed record ClickPoint(int X, int Y)
{
    public static ClickPoint Parse(string value)
    {
        var parts = value.Split(',', StringSplitOptions.TrimEntries);
        if (parts.Length != 2 || !int.TryParse(parts[0], out var x) ||
            !int.TryParse(parts[1], out var y) || x < 0 || y < 0)
        {
            throw new ArgumentException("--click-after-ready requires non-negative integer x,y coordinates.");
        }
        return new ClickPoint(x, y);
    }
}
