namespace ChromiumBaseline;

// Trusted CDP control-key replay; does not call an author's event handlers directly.
internal sealed record ControlKey(string Key, string Code, int VirtualKey, string Text)
{
    public static ControlKey Parse(string value) => value switch
    {
        "Escape,Escape" => new("Escape", "Escape", 27, ""),
        "Enter,Enter" => new("Enter", "Enter", 13, "\r"),
        "Tab,Tab" => new("Tab", "Tab", 9, ""),
        " ,Space" => new(" ", "Space", 32, " "),
        "ArrowLeft,ArrowLeft" => new("ArrowLeft", "ArrowLeft", 37, ""),
        "ArrowUp,ArrowUp" => new("ArrowUp", "ArrowUp", 38, ""),
        "ArrowRight,ArrowRight" => new("ArrowRight", "ArrowRight", 39, ""),
        "ArrowDown,ArrowDown" => new("ArrowDown", "ArrowDown", 40, ""),
        _ => throw new ArgumentException("--key-after-ready requires a supported matching control key,code pair.")
    };

    public async Task<int> DispatchAsync(CdpConnection cdp, TimeSpan timeout, int nextId)
    {
        // Input.dispatchKeyEvent synthesizes normal browser default actions as
        // well as key events; keyDown text is omitted on keyUp.
        // https://chromedevtools.github.io/devtools-protocol/tot/Input/#method-dispatchKeyEvent
        foreach (var type in new[] { "keyDown", "keyUp" })
        {
            await cdp.CallAsync(nextId++, "Input.dispatchKeyEvent", new
            {
                type, key = Key, code = Code, windowsVirtualKeyCode = VirtualKey,
                text = type == "keyDown" ? Text : "",
                unmodifiedText = type == "keyDown" ? Text : ""
            }, timeout);
        }
        return nextId;
    }
}
