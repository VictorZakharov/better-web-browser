using ChromiumBaseline;

internal static class ChromiumProfileTests
{
    public static void Run()
    {
        var target = ChromiumProfile.RepositoryTarget();
        var root = Path.Combine(target, $"chromium-profile-tests-{Guid.NewGuid():N}");
        var owned = Path.Combine(root, "owned");
        var unowned = Path.Combine(root, "unowned");
        Assert(ChromiumProfile.ValidatePath(owned) == Path.GetFullPath(owned),
            "dedicated target profile was rejected");
        Reject<ArgumentException>(() => ChromiumProfile.ValidatePath(target),
            "target directory was accepted as a profile");
        Reject<ArgumentException>(() => ChromiumProfile.ValidatePath(root),
            "single target child was accepted as a profile");
        Reject<ArgumentException>(() => ChromiumProfile.ValidatePath(Path.GetTempPath()),
            "path outside repository target was accepted");
        Reject<ArgumentException>(() => ChromiumProfile.ValidatePath("relative-profile"),
            "relative profile path was accepted");

        var executable = typeof(Options).Assembly.Location;
        var defaults = Options.Parse(new[] {
            "--url", "about:blank", "--output", "unused.json", "--chrome", executable
        });
        Assert(defaults.ProfileDirectory is null && defaults.CacheDisabled,
            "fresh-profile/cache-disabled defaults changed");
        var warm = Options.Parse(new[] {
            "--url", "about:blank", "--output", "unused.json", "--chrome", executable,
            "--profile-directory", owned, "--enable-cache"
        });
        Assert(warm.ProfileDirectory == Path.GetFullPath(owned) && !warm.CacheDisabled,
            "warm-profile/cache-enabled options were not parsed");

        try
        {
            using (var lease = ChromiumProfile.Open(owned))
            {
                Assert(lease.Path == Path.GetFullPath(owned), "owned profile path changed");
                Assert(File.Exists(Path.Combine(owned, ".chromium-baseline-profile")),
                    "profile ownership marker is missing");
                Reject<IOException>(() => ChromiumProfile.Open(owned),
                    "simultaneous use of one profile was accepted");
            }
            using (ChromiumProfile.Open(owned)) { }
            Directory.CreateDirectory(unowned);
            File.WriteAllText(Path.Combine(unowned, "Preferences"), "user data");
            Reject<InvalidOperationException>(() => ChromiumProfile.Open(unowned),
                "unmarked populated profile was accepted");
        }
        finally
        {
            // This test owns a unique child under the verified repository target.
            if (Path.GetDirectoryName(root) != target)
                throw new InvalidOperationException("Refusing to remove an unexpected test profile directory.");
            if (Directory.Exists(root)) Directory.Delete(root, recursive: true);
        }
        Console.WriteLine("Chromium profile option and ownership tests passed.");
    }

    public static async Task RunWarmCaptureAsync(string chrome)
    {
        var target = ChromiumProfile.RepositoryTarget();
        var root = Path.Combine(target, $"chromium-warm-test-{Guid.NewGuid():N}");
        var profile = Path.Combine(root, "profile");
        try
        {
            for (var run = 1; run <= 2; run++)
            {
                var result = await ChromeRun.ExecuteAsync(new Options {
                    Url = "data:text/html,%3C!doctype%20html%3E%3Ctitle%3EWarm%20profile%3C%2Ftitle%3E%3Ch1%3EWarm%20profile%3C%2Fh1%3E",
                    Output = Path.Combine(root, $"run-{run}.json"),
                    ChromePath = chrome,
                    ProfileDirectory = profile,
                    CacheDisabled = false,
                    ViewportWidth = 640,
                    ViewportHeight = 400,
                    SettleMs = 100,
                    TimeoutMs = 15_000
                });
                Assert(result.Error is null && result.CleanupError is null,
                    $"warm-profile capture {run} failed: {result.Error}; {result.CleanupError}");
                Assert(!result.FreshProfile && !result.CacheDisabled,
                    $"warm-profile capture {run} reported incorrect mode flags");
                Assert(Directory.Exists(profile), "warm profile was removed after capture");
            }
        }
        finally
        {
            if (Path.GetDirectoryName(root) != target)
                throw new InvalidOperationException("Refusing to remove an unexpected warm test directory.");
            if (Directory.Exists(root)) Directory.Delete(root, recursive: true);
        }
        Console.WriteLine("Chromium repeated warm-profile captures passed.");
    }

    private static void Reject<TException>(Action action, string message) where TException : Exception
    {
        try { action(); }
        catch (TException) { return; }
        throw new InvalidOperationException(message);
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
