using ChromiumBaseline;

internal static class ControlKeyTests
{
    public static void Run()
    {
        foreach (var (pair, key, code, value) in new[]
        {
            ("Escape,Escape", "Escape", "Escape", 27),
            ("Enter,Enter", "Enter", "Enter", 13),
            ("Tab,Tab", "Tab", "Tab", 9),
            (" ,Space", " ", "Space", 32),
            ("ArrowDown,ArrowDown", "ArrowDown", "ArrowDown", 40)
        })
        {
            var parsed = ControlKey.Parse(pair);
            if (parsed.Key != key || parsed.Code != code || parsed.VirtualKey != value)
                throw new InvalidOperationException("Control key changed its browser identity.");
        }
        foreach (var malformed in new[] { "", "Escape", "Escape,KeyE", "Escape,Escape,27", "Escape,Escape\0", "a,KeyA" })
        {
            try { ControlKey.Parse(malformed); }
            catch (ArgumentException) { continue; }
            throw new InvalidOperationException("Unsupported or malformed control key accepted.");
        }
    }
}
