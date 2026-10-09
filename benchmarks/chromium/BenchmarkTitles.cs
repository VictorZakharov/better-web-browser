using System.Text;

namespace ChromiumBaseline;

// The same 512-byte UTF-8 prefix contract as Breeze's diagnostic document title.
// Read after the measured settle interval; title extraction is not a readiness gate.
internal sealed record BenchmarkTitles(string DocumentTitle, bool DocumentTitleTruncated)
{
    public static BenchmarkTitles From(string source, bool sourceTruncated)
    {
        const int maximumBytes = 512;
        var bytes = 0;
        var prefix = new StringBuilder();
        foreach (var rune in source.EnumerateRunes())
        {
            if (bytes + rune.Utf8SequenceLength > maximumBytes)
                return new BenchmarkTitles(prefix.ToString(), true);
            bytes += rune.Utf8SequenceLength;
            prefix.Append(rune.ToString());
        }
        return new BenchmarkTitles(prefix.ToString(), sourceTruncated);
    }
}
