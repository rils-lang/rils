#nullable enable
using System;

namespace Rils.CSharp
{
    public sealed class RilsException : Exception
    {
        internal RilsException(
            RilsStatus status,
            string message,
            string sourceName,
            ulong spanStart,
            ulong spanEnd)
            : base(message)
        {
            Status = status;
            SourceName = sourceName;
            SpanStart = spanStart;
            SpanEnd = spanEnd;
        }

        public RilsStatus Status { get; }

        public string SourceName { get; }

        public ulong SpanStart { get; }

        public ulong SpanEnd { get; }

        public override string ToString()
        {
            string location = string.IsNullOrEmpty(SourceName)
                ? string.Empty
                : $" ({SourceName}:{SpanStart}..{SpanEnd})";
            return $"{Status}: {Message}{location}";
        }
    }
}
