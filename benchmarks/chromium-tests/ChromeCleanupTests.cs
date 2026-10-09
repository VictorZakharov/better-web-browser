using ChromiumBaseline;

internal static class ChromeCleanupTests
{
    internal static async Task RunAsync()
    {
        var temporaryRoot = Path.GetFullPath(Path.GetTempPath());
        var name = "breeze-chromium-" + Guid.NewGuid().ToString("N");
        var profile = Path.Combine(temporaryRoot, name);
        Check(ChromeCleanup.ValidateFreshProfile(profile, temporaryRoot) == profile,
            "fresh GUID profile was not admitted");
        Check(ChromeCleanup.ValidateFreshProfile(profile + Path.DirectorySeparatorChar, temporaryRoot)
            .TrimEnd(Path.DirectorySeparatorChar) == profile, "trailing separator changed profile ownership");
        foreach (var invalid in new[] {
            temporaryRoot,
            Path.Combine(temporaryRoot, "breeze-chromium-"),
            Path.Combine(temporaryRoot, "breeze-chromium-user-profile"),
            Path.Combine(temporaryRoot, name + "-backup"),
            Path.Combine(temporaryRoot, name, "Default"),
            Path.Combine(temporaryRoot, "nested", name),
            Path.Combine(temporaryRoot + "-sibling", name),
            Path.Combine(temporaryRoot, "..", name),
            Path.Combine(temporaryRoot, "breeze-chromium-" + Guid.NewGuid().ToString("D")),
            Path.Combine(temporaryRoot, "breeze-chromium-" + new string('x', 32))
        })
        {
            var rejected = false;
            try { ChromeCleanup.ValidateFreshProfile(invalid, temporaryRoot); }
            catch (InvalidOperationException) { rejected = true; }
            Check(rejected, "unexpected cleanup target was accepted: " + invalid);
        }

        // Missing owned profiles are harmless and do not create anything.
        await ChromeCleanup.DeleteFreshProfileAsync(profile);
        Check(!Directory.Exists(profile), "cleanup created a missing profile");
        Directory.CreateDirectory(Path.Combine(profile, "Default"));
        var file = Path.Combine(profile, "Default", "Account Web Data");
        await File.WriteAllTextAsync(file, "task-owned SQLite cleanup control");
        FileStream? held = null;
        try
        {
            if (OperatingSystem.IsWindows())
            {
                held = new FileStream(file, FileMode.Open, FileAccess.Read, FileShare.None);
                var cleanup = ChromeCleanup.DeleteFreshProfileAsync(profile);
                Check(!cleanup.IsCompleted, "locked file was silently reported removed");
                await Task.Delay(150);
                held.Dispose();
                held = null;
                await cleanup.WaitAsync(TimeSpan.FromSeconds(5));
            }
            else await ChromeCleanup.DeleteFreshProfileAsync(profile);
            Check(!Directory.Exists(profile), "unlocked owned profile was left behind");
        }
        finally
        {
            held?.Dispose();
            await ChromeCleanup.DeleteFreshProfileAsync(profile);
        }
        Check(Directory.Exists(temporaryRoot), "cleanup removed the containing temporary directory");
        Console.WriteLine("Chromium cleanup ownership and delayed-handle tests passed.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
