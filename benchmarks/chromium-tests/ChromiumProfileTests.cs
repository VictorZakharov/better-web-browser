using ChromiumBaseline;

internal static class ChromiumProfileTests
{
    public static void Run()
    {
        RunCleanupPolicyTests();
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

        Exception? failure = null;
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
        catch (Exception error)
        {
            failure = error;
            throw;
        }
        finally
        {
            DeleteOwnedTestDirectory(root, target, failure);
        }
        Console.WriteLine("Chromium profile option and ownership tests passed.");
    }

    public static async Task RunWarmCaptureAsync(string chrome)
    {
        var target = ChromiumProfile.RepositoryTarget();
        var root = Path.Combine(target, $"chromium-warm-test-{Guid.NewGuid():N}");
        var profile = Path.Combine(root, "profile");
        Exception? failure = null;
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
        catch (Exception error)
        {
            failure = error;
            throw;
        }
        finally
        {
            DeleteOwnedTestDirectory(root, target, failure);
        }
        Console.WriteLine("Chromium repeated warm-profile captures passed.");
    }

    private static void DeleteOwnedTestDirectory(string root, string target, Exception? failure = null,
        Action<string>? remove = null, Action<int>? delay = null)
    {
        try
        {
            // This test owns only its unique direct child of the repository target.
            if (!Path.IsPathFullyQualified(root) || !Path.IsPathFullyQualified(target) ||
                !string.Equals(Path.GetDirectoryName(Path.GetFullPath(root)), Path.GetFullPath(target),
                    StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("Refusing to remove an unexpected test profile directory.");
            var full = Path.GetFullPath(root);
            remove ??= path => { if (Directory.Exists(path)) Directory.Delete(path, recursive: true); };
            delay ??= Thread.Sleep;
            // Match ChromeRun's fresh-profile policy. Awaiting the Chrome root after
            // Kill(entireProcessTree:true) does not await every descendant's file handles.
            // https://learn.microsoft.com/dotnet/api/system.diagnostics.process.kill
            for (var attempt = 0; attempt < 5; attempt++)
            {
                try
                {
                    remove(full);
                    return;
                }
                catch (Exception error) when (attempt < 4 && error is IOException or UnauthorizedAccessException)
                {
                    delay(100);
                }
            }
        }
        catch (Exception cleanupError) when (failure is not null)
        {
            // A finally failure must not replace the capture/assertion that triggered it.
            throw new AggregateException($"Test failed and owned profile cleanup failed: {root}",
                failure, cleanupError);
        }
    }

    private static void RunCleanupPolicyTests()
    {
        var target = ChromiumProfile.RepositoryTarget();
        var root = Path.Combine(target, $"chromium-cleanup-tests-{Guid.NewGuid():N}");
        var calls = 0;
        var waits = new List<int>();
        DeleteOwnedTestDirectory(root, target, remove: path =>
        {
            Assert(path == Path.GetFullPath(root), "cleanup changed its owned directory");
            calls++;
            if (calls == 1) throw new IOException("descendant still owns a database");
            if (calls == 2) throw new UnauthorizedAccessException("profile handle is retiring");
        }, delay: waits.Add);
        Assert(calls == 3 && waits.SequenceEqual(new[] { 100, 100 }),
            "transient profile cleanup did not reuse the bounded retry policy");

        calls = 0;
        waits.Clear();
        var persistent = new IOException("profile remains in use");
        var observed = Capture<IOException>(() => DeleteOwnedTestDirectory(root, target,
            remove: _ => { calls++; throw persistent; }, delay: waits.Add));
        Assert(ReferenceEquals(observed, persistent) && calls == 5 &&
            waits.SequenceEqual(new[] { 100, 100, 100, 100 }),
            "permanent cleanup failure was suppressed or retried without a bound");

        calls = 0;
        waits.Clear();
        var unexpected = new InvalidOperationException("invalid filesystem operation");
        var unhandled = Capture<InvalidOperationException>(() => DeleteOwnedTestDirectory(root, target,
            remove: _ => { calls++; throw unexpected; }, delay: waits.Add));
        Assert(ReferenceEquals(unhandled, unexpected) && calls == 1 && waits.Count == 0,
            "unexpected cleanup failure was retried");

        var primary = new InvalidOperationException("warm capture failed first");
        var combined = Capture<AggregateException>(() => DeleteOwnedTestDirectory(root, target, primary,
            remove: _ => throw persistent, delay: _ => { }));
        Assert(combined.InnerExceptions.Count == 2 &&
            ReferenceEquals(combined.InnerExceptions[0], primary) &&
            ReferenceEquals(combined.InnerExceptions[1], persistent),
            "cleanup masked the original warm capture failure");
        calls = 0;
        DeleteOwnedTestDirectory(root, target, primary, remove: _ => calls++,
            delay: _ => throw new InvalidOperationException("successful cleanup must not wait"));
        Assert(calls == 1, "successful cleanup did not complete on its first attempt");
        calls = 0;
        Capture<InvalidOperationException>(() => DeleteOwnedTestDirectory(target, target,
            remove: _ => calls++, delay: _ => { }));
        Assert(calls == 0, "cleanup attempted to remove the repository target");
        Console.WriteLine("Chromium bounded owned-profile cleanup policy tests passed without browser execution.");
    }

    private static TException Capture<TException>(Action action) where TException : Exception
    {
        try { action(); }
        catch (TException error) { return error; }
        throw new InvalidOperationException($"Expected {typeof(TException).Name} was not raised.");
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
