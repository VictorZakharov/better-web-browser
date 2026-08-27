namespace ChromiumBaseline;

internal static class PageClassification
{
    public const string EmptyDocumentError = "Chromium produced a blank or structurally empty document.";
    public const string BrowserError = "Chromium displayed a browser error surface.";

    public static string? Classify(BenchmarkResult result)
    {
        if (result.BrowserErrorSurface)
        {
            return BrowserError;
        }

        var lightDomContent = result.BodyTextLength >= 20 && result.ElementCount >= 5;
        var composedContent = result.ComposedTextLength >= 20 && result.ComposedElementCount >= 5;
        var accessibleContent = result.AccessibilityTextLength >= 20 && result.AccessibilityNodeCount >= 3;
        var paintedContent = result.PaintedPixelRatio >= 0.001;
        return lightDomContent || composedContent || accessibleContent || paintedContent
            ? null
            : EmptyDocumentError;
    }
}
