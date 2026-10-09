using System.Diagnostics;

namespace ChromiumBaseline;

// Teardown is outside the measurement interval. Retain the original process
// handles while waiting: a later reuse of a child PID is not our process.
internal static class ChromeCleanup
{
    internal static async Task StopAsync(Process root)
    {
        var children = new List<Process>();
        try
        {
            foreach (var id in ProcessTree.Descendants(root.Id))
            {
                if (id == root.Id) continue;
                Process? child = null;
                try
                {
                    child = Process.GetProcessById(id);
                    _ = child.SafeHandle;
                    if (child.HasExited) child.Dispose();
                    else children.Add(child);
                }
                catch (Exception error) when (error is ArgumentException or
                    InvalidOperationException or System.ComponentModel.Win32Exception)
                {
                    child?.Dispose();
                }
            }
            if (!root.HasExited) root.Kill(entireProcessTree: true);
            // .NET WaitForExit on the root does not wait for killed descendants.
            // https://learn.microsoft.com/dotnet/api/system.diagnostics.process.kill
            await Task.WhenAll(children.Append(root).Select(process => process.WaitForExitAsync()))
                .WaitAsync(TimeSpan.FromSeconds(5));
        }
        finally
        {
            foreach (var child in children) child.Dispose();
        }
    }

    internal static string ValidateFreshProfile(string profile, string temporaryRoot)
    {
        var full = Path.GetFullPath(profile);
        var parent = Path.GetDirectoryName(Path.TrimEndingDirectorySeparator(full));
        var expected = Path.TrimEndingDirectorySeparator(Path.GetFullPath(temporaryRoot));
        var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        var name = Path.GetFileName(Path.TrimEndingDirectorySeparator(full));
        const string prefix = "breeze-chromium-";
        if (!string.Equals(parent, expected, comparison) || !name.StartsWith(prefix, StringComparison.Ordinal) ||
            !Guid.TryParseExact(name[prefix.Length..], "N", out _))
            throw new InvalidOperationException("Refusing to remove an unexpected Chromium profile path.");
        if (Directory.Exists(full) && (File.GetAttributes(full) & FileAttributes.ReparsePoint) != 0)
            throw new InvalidOperationException("Refusing to remove a redirected Chromium profile directory.");
        return full;
    }

    internal static async Task DeleteFreshProfileAsync(string profile)
    {
        var full = ValidateFreshProfile(profile, Path.GetTempPath());
        for (var attempt = 0; ; attempt++)
        {
            try
            {
                if (Directory.Exists(full)) Directory.Delete(full, recursive: true);
                return;
            }
            catch (Exception error) when (attempt < 20 && error is IOException or UnauthorizedAccessException)
            {
                // File-system/security services may release a closed SQLite
                // handle shortly after all owned Chrome processes have exited.
                await Task.Delay(100);
            }
        }
    }
}
