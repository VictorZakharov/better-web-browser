using System.Diagnostics;
using System.Runtime.InteropServices;

namespace ChromiumBaseline;

internal readonly record struct ProcessSample(
    long WorkingSetBytes,
    long PrivateBytes,
    long PeakWorkingSetBytes,
    double CpuTimeMs,
    int ProcessCount);

internal static class ProcessTree
{
    private const uint SnapshotProcesses = 0x00000002;
    private static readonly IntPtr InvalidHandle = new(-1);

    public static ProcessSample Sample(int rootProcessId)
    {
        long workingSet = 0;
        long privateBytes = 0;
        long peakWorkingSet = 0;
        double cpu = 0;
        var count = 0;
        foreach (var processId in Descendants(rootProcessId))
        {
            try
            {
                using var process = Process.GetProcessById(processId);
                workingSet += process.WorkingSet64;
                privateBytes += process.PrivateMemorySize64;
                peakWorkingSet += process.PeakWorkingSet64;
                cpu += process.TotalProcessorTime.TotalMilliseconds;
                count++;
            }
            catch (Exception error) when (error is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception)
            {
                // The process exited or became inaccessible between snapshots.
            }
        }
        return new ProcessSample(workingSet, privateBytes, peakWorkingSet, cpu, count);
    }

    public static bool HasVisibleWindow(int rootProcessId)
    {
        return Descendants(rootProcessId).Any(processId =>
        {
            try
            {
                using var process = Process.GetProcessById(processId);
                return process.MainWindowHandle != IntPtr.Zero;
            }
            catch (Exception error) when (error is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception)
            {
                return false;
            }
        });
    }

    private static HashSet<int> Descendants(int rootProcessId)
    {
        var parents = SnapshotParentMap();
        var started = new Dictionary<int, long?>();
        long? CreationTime(int id)
        {
            if (started.TryGetValue(id, out var cached)) return cached;
            try
            {
                using var process = Process.GetProcessById(id);
                return started[id] = process.StartTime.ToUniversalTime().Ticks;
            }
            catch (Exception error) when (error is ArgumentException or InvalidOperationException or System.ComponentModel.Win32Exception)
            {
                return started[id] = null;
            }
        }
        return SelectDescendants(rootProcessId, parents, CreationTime);
    }

    // Windows parent IDs outlive their parent and can point to a recycled, unrelated PID.
    // Never attribute an older process (or its descendants) to a new benchmark process.
    // https://devblogs.microsoft.com/oldnewthing/20200122-00/?p=103355
    internal static HashSet<int> SelectDescendants(
        int rootProcessId, IReadOnlyDictionary<int, int> parents, Func<int, long?> creationTime)
    {
        var result = new HashSet<int> { rootProcessId };
        var changed = true;
        while (changed)
        {
            changed = false;
            foreach (var (processId, parentId) in parents)
            {
                if (!result.Contains(processId) && result.Contains(parentId)
                    && creationTime(parentId) is { } parentStarted
                    && creationTime(processId) is { } childStarted
                    && childStarted >= parentStarted)
                {
                    result.Add(processId);
                    changed = true;
                }
            }
        }
        return result;
    }

    private static Dictionary<int, int> SnapshotParentMap()
    {
        var result = new Dictionary<int, int>();
        var snapshot = CreateToolhelp32Snapshot(SnapshotProcesses, 0);
        if (snapshot == InvalidHandle)
        {
            return result;
        }
        try
        {
            var entry = new ProcessEntry32 { Size = (uint)Marshal.SizeOf<ProcessEntry32>() };
            if (!Process32First(snapshot, ref entry))
            {
                return result;
            }
            do
            {
                result[(int)entry.ProcessId] = (int)entry.ParentProcessId;
                entry.Size = (uint)Marshal.SizeOf<ProcessEntry32>();
            } while (Process32Next(snapshot, ref entry));
        }
        finally
        {
            CloseHandle(snapshot);
        }
        return result;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ProcessEntry32
    {
        public uint Size;
        public uint Usage;
        public uint ProcessId;
        public UIntPtr DefaultHeapId;
        public uint ModuleId;
        public uint Threads;
        public uint ParentProcessId;
        public int BasePriority;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string ExecutableFile;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool Process32First(IntPtr snapshot, ref ProcessEntry32 entry);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool Process32Next(IntPtr snapshot, ref ProcessEntry32 entry);
    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);
}
