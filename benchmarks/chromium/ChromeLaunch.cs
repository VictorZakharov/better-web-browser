using System.Diagnostics;
using System.Globalization;

namespace ChromiumBaseline;

internal static class ChromeLaunch
{
    public static Process Start(Options options, string profile)
        => Process.Start(Settings(options, profile)) ?? throw new InvalidOperationException("Chromium did not start.");

    internal static ProcessStartInfo Settings(Options options, string profile)
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
            "--mute-audio",
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
        return start;
    }
}
