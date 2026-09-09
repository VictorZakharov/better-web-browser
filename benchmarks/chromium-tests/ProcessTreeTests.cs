using ChromiumBaseline;

internal static class ProcessTreeTests
{
    public static void Run()
    {
        var parents = new Dictionary<int, int> {
            [1] = 99, [2] = 1, [3] = 2, [4] = 1, [5] = 4, [6] = 2, [7] = 1
        };
        var times = new Dictionary<int, long> {
            [1] = 100, [2] = 101, [3] = 102,
            [4] = 50, [5] = 103, // Old process names a recycled parent; its subtree is not ours.
            [6] = 100, // Recycled intermediate parent, despite not being older than the root.
            // Process 7 disappeared or cannot be identified.
        };
        var actual = ProcessTree.SelectDescendants(1, parents,
            id => times.TryGetValue(id, out var time) ? time : null);
        if (!actual.SetEquals(new[] { 1, 2, 3 }))
            throw new InvalidOperationException("Process tree accepted recycled or unknown process identities.");
        var simultaneous = ProcessTree.SelectDescendants(1, parents, _ => 100);
        if (!simultaneous.SetEquals(new[] { 1, 2, 3, 4, 5, 6, 7 }))
            throw new InvalidOperationException("Process tree rejected equal-resolution creation timestamps.");
        Console.WriteLine("Process identity attribution tests passed.");
    }
}
