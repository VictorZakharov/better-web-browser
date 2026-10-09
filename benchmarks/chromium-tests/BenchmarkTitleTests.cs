using ChromiumBaseline;
using System.Text.Json;

internal static class BenchmarkTitleTests
{
    public static void Run()
    {
        Check(BenchmarkTitles.From("done 12.3;pixel=0,255,0,255", false) is
            { DocumentTitle: "done 12.3;pixel=0,255,0,255", DocumentTitleTruncated: false },
            "ordinary fixture title was lost or marked truncated");
        Check(BenchmarkTitles.From(new string('a', 512), false) is
            { DocumentTitle.Length: 512, DocumentTitleTruncated: false }, "exact byte limit failed");
        Check(BenchmarkTitles.From(new string('a', 511) + "☃", false) is
            { DocumentTitle.Length: 511, DocumentTitleTruncated: true }, "UTF-8 scalar was split");
        Check(BenchmarkTitles.From(new string('a', 510) + "😀", false) is
            { DocumentTitle.Length: 510, DocumentTitleTruncated: true }, "surrogate pair was split");
        Check(BenchmarkTitles.From("prefix", true).DocumentTitleTruncated,
            "bounded browser-side sampling lost the truncation flag");
        var quoted = BenchmarkTitles.From("quote\" newline\nslash\\", false);
        using var encoded = JsonDocument.Parse(JsonSerializer.Serialize(quoted, JsonDefaults.Options));
        Check(encoded.RootElement.GetProperty("document_title").GetString() == quoted.DocumentTitle,
            "title report bypassed normal JSON escaping");
        Console.WriteLine("Bounded document-title diagnostics tests passed without browser execution.");
    }

    private static void Check(bool success, string message)
    {
        if (!success) throw new InvalidOperationException(message);
    }
}
