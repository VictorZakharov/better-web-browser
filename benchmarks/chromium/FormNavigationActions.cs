using System.Text.Json;

namespace ChromiumBaseline;

// Trusted editing and history traversal, not script-driven .value/.submit shortcuts.
internal static class FormNavigationActions
{
    public static async Task<int> RunAsync(CdpConnection cdp, Options options, TimeSpan timeout, int nextId)
    {
        if (options.BackAfterReady)
        {
            await Task.Delay(options.NavigationDelayMs);
            var history = await cdp.CallAsync(nextId++, "Page.getNavigationHistory", new { }, timeout);
            var index = history.GetProperty("currentIndex").GetInt32();
            if (index < 1) throw new InvalidOperationException("Chromium Back has no previous entry.");
            var entry = history.GetProperty("entries")[index - 1].GetProperty("id").GetInt32();
            await cdp.CallAsync(nextId++, "Page.navigateToHistoryEntry", new { entryId = entry }, timeout);
        }
        if (options.SubmitControlSelector is { } selector)
        {
            if (options.SubmitControlValue is not { } text)
                throw new ArgumentException("--submit-control-selector requires --submit-control-value.");
            await Task.Delay(options.NavigationDelayMs);
            var selected = await BrowserActions.EvaluateAsync(cdp, nextId++, $$"""
                (() => {
                  const control = document.querySelector({{JsonSerializer.Serialize(selector)}});
                  if (!control || typeof control.select !== 'function') return false;
                  control.focus(); control.select(); return true;
                })()
                """, timeout);
            if (selected.ValueKind != JsonValueKind.True)
                throw new InvalidOperationException("Chromium editable control was not found.");
            await cdp.CallAsync(nextId++, "Input.insertText", new { text }, timeout);
            var navigation = cdp.ReadUntilAsync(root =>
                root.TryGetProperty("method", out var method) && method.GetString() == "Page.loadEventFired", timeout);
            foreach (var type in new[] { "keyDown", "keyUp" })
                await cdp.CallAsync(nextId++, "Input.dispatchKeyEvent", new
                { type, key = "Enter", code = "Enter", windowsVirtualKeyCode = 13,
                  text = type == "keyDown" ? "\r" : "", unmodifiedText = type == "keyDown" ? "\r" : "" }, timeout);
            await navigation;
        }
        return nextId;
    }
}
