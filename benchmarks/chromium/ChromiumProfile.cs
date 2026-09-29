namespace ChromiumBaseline;

// Reused profiles are opt-in and owned by this harness, never an installed
// browser's default profile. Fresh profiles retain their existing temp cleanup.
internal static class ChromiumProfile
{
    private const string MarkerName = ".chromium-baseline-profile";
    private const string MarkerContents = "ChromiumBaseline dedicated profile v1\n";

    internal static string ValidatePath(string requested) => ValidatePath(requested, RepositoryTarget());

    internal static string ValidatePath(string requested, string targetRoot)
    {
        if (string.IsNullOrWhiteSpace(requested) || !Path.IsPathFullyQualified(requested))
            throw new ArgumentException("--profile-directory requires an absolute path under this repository's target directory.");

        var target = Path.GetFullPath(targetRoot);
        var profile = Path.GetFullPath(requested);
        var relative = Path.GetRelativePath(target, profile);
        var components = relative.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        if (Path.IsPathRooted(relative) || components.Length < 2 ||
            components.Any(component => component is "" or "." or ".."))
        {
            throw new ArgumentException("--profile-directory must be a dedicated child at least two levels below this repository's target directory.");
        }
        RejectReparsePoints(target, components);
        return profile;
    }

    internal static ProfileLease Open(string requested) => Open(requested, RepositoryTarget());

    internal static ProfileLease Open(string requested, string targetRoot)
    {
        var profile = ValidatePath(requested, targetRoot);
        Directory.CreateDirectory(profile);
        // Check again after creation in case an existing component redirected it.
        profile = ValidatePath(profile, targetRoot);
        var marker = Path.Combine(profile, MarkerName);
        if (File.Exists(marker))
        {
            if (File.ReadAllText(marker) != MarkerContents)
                throw new InvalidOperationException("Chromium profile ownership marker is invalid.");
        }
        else
        {
            if (Directory.EnumerateFileSystemEntries(profile).Any())
                throw new InvalidOperationException("Refusing a nonempty Chromium profile without this harness's ownership marker.");
            File.WriteAllText(marker, MarkerContents);
        }
        // Fail if another benchmark still owns the same profile.
        return new ProfileLease(profile, File.Open(marker, FileMode.Open, FileAccess.Read, FileShare.None));
    }

    private static void RejectReparsePoints(string target, string[] components)
    {
        var current = target;
        if (Directory.Exists(current) && (File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
            throw new ArgumentException("--profile-directory cannot traverse a redirected target directory.");
        foreach (var component in components)
        {
            current = Path.Combine(current, component);
            if (Directory.Exists(current) && (File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
                throw new ArgumentException("--profile-directory cannot traverse a junction or symbolic link.");
        }
    }

    internal static string RepositoryTarget()
    {
        foreach (var start in new[] { AppContext.BaseDirectory, Directory.GetCurrentDirectory() })
        {
            for (var directory = new DirectoryInfo(start); directory is not null; directory = directory.Parent)
            {
                if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml")) &&
                    File.Exists(Path.Combine(directory.FullName, "benchmarks", "chromium", "ChromiumBaseline.csproj")))
                    return Path.Combine(directory.FullName, "target");
            }
        }
        throw new InvalidOperationException("Could not locate the repository target directory for a dedicated Chromium profile.");
    }

    internal sealed class ProfileLease(string path, FileStream markerLock) : IDisposable
    {
        internal string Path { get; } = path;
        public void Dispose() => markerLock.Dispose();
    }
}
