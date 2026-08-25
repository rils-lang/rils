#nullable enable
using System;

namespace Rils.CSharp
{
    public readonly struct RilsInt128 : IEquatable<RilsInt128>
    {
        public RilsInt128(ulong low, long high)
        {
            Low = low;
            High = high;
        }

        public ulong Low { get; }
        public long High { get; }

        public static RilsInt128 FromInt64(long value) =>
            new RilsInt128(unchecked((ulong)value), value < 0 ? -1L : 0L);

        public bool Equals(RilsInt128 other) => Low == other.Low && High == other.High;
        public override bool Equals(object? obj) => obj is RilsInt128 other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Low, High);
        public static bool operator ==(RilsInt128 left, RilsInt128 right) => left.Equals(right);
        public static bool operator !=(RilsInt128 left, RilsInt128 right) => !left.Equals(right);
    }

    public readonly struct RilsUInt128 : IEquatable<RilsUInt128>
    {
        public RilsUInt128(ulong low, ulong high)
        {
            Low = low;
            High = high;
        }

        public ulong Low { get; }
        public ulong High { get; }

        public bool Equals(RilsUInt128 other) => Low == other.Low && High == other.High;
        public override bool Equals(object? obj) => obj is RilsUInt128 other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Low, High);
        public static bool operator ==(RilsUInt128 left, RilsUInt128 right) => left.Equals(right);
        public static bool operator !=(RilsUInt128 left, RilsUInt128 right) => !left.Equals(right);
    }

    public readonly struct RilsChar : IEquatable<RilsChar>
    {
        public RilsChar(uint value)
        {
            if (value > 0x10FFFF || value >= 0xD800 && value <= 0xDFFF)
            {
                throw new ArgumentOutOfRangeException(nameof(value), "Value must be a Unicode scalar value.");
            }
            Value = value;
        }

        public uint Value { get; }

        public bool Equals(RilsChar other) => Value == other.Value;
        public override bool Equals(object? obj) => obj is RilsChar other && Equals(other);
        public override int GetHashCode() => unchecked((int)Value);
        public override string ToString() => char.ConvertFromUtf32(checked((int)Value));
        public static bool operator ==(RilsChar left, RilsChar right) => left.Equals(right);
        public static bool operator !=(RilsChar left, RilsChar right) => !left.Equals(right);
    }

    /// Canonical 16-byte inline host payload. Components are packed explicitly
    /// as little-endian IEEE-754 values; this is not a managed struct layout.
    public readonly struct RilsInlineValue : IEquatable<RilsInlineValue>
    {
        internal RilsInlineValue(ulong low, ulong high)
        {
            Low = low;
            High = high;
        }

        internal ulong Low { get; }
        internal ulong High { get; }

        public static RilsInlineValue FromF32(float x, float y) =>
            FromF32Bits(x, y, 0f, 0f, 2);

        public static RilsInlineValue FromF32(float x, float y, float z) =>
            FromF32Bits(x, y, z, 0f, 3);

        public static RilsInlineValue FromF32(float x, float y, float z, float w) =>
            FromF32Bits(x, y, z, w, 4);

        public float GetF32(int index)
        {
            if ((uint)index >= 4) throw new ArgumentOutOfRangeException(nameof(index));
            ulong source = index < 2 ? Low : High;
            int shift = (index & 1) * 32;
            return BitConverter.Int32BitsToSingle(unchecked((int)(source >> shift)));
        }

        private static RilsInlineValue FromF32Bits(
            float x, float y, float z, float w, int componentCount)
        {
            uint xBits = unchecked((uint)BitConverter.SingleToInt32Bits(x));
            uint yBits = unchecked((uint)BitConverter.SingleToInt32Bits(y));
            uint zBits = componentCount >= 3
                ? unchecked((uint)BitConverter.SingleToInt32Bits(z))
                : 0;
            uint wBits = componentCount >= 4
                ? unchecked((uint)BitConverter.SingleToInt32Bits(w))
                : 0;
            return new RilsInlineValue(
                xBits | (ulong)yBits << 32,
                zBits | (ulong)wBits << 32);
        }

        public bool Equals(RilsInlineValue other) => Low == other.Low && High == other.High;
        public override bool Equals(object? obj) => obj is RilsInlineValue other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Low, High);
        public static bool operator ==(RilsInlineValue left, RilsInlineValue right) => left.Equals(right);
        public static bool operator !=(RilsInlineValue left, RilsInlineValue right) => !left.Equals(right);
    }

    /// Allocation-free field writer for the canonical packed InlineValue ABI.
    public struct RilsInlineValueWriter
    {
        private ulong _low;
        private ulong _high;
        private int _offset;

        public int BytesWritten => _offset;

        public void WriteBool(bool value) => WriteUnsigned(value ? 1UL : 0UL, 1);
        public void WriteI8(sbyte value) => WriteUnsigned(unchecked((byte)value), 1);
        public void WriteI16(short value) => WriteUnsigned(unchecked((ushort)value), 2);
        public void WriteI32(int value) => WriteUnsigned(unchecked((uint)value), 4);
        public void WriteI64(long value) => WriteUnsigned(unchecked((ulong)value), 8);
        public void WriteU8(byte value) => WriteUnsigned(value, 1);
        public void WriteU16(ushort value) => WriteUnsigned(value, 2);
        public void WriteU32(uint value) => WriteUnsigned(value, 4);
        public void WriteU64(ulong value) => WriteUnsigned(value, 8);
        public void WriteF32(float value) =>
            WriteUnsigned(unchecked((uint)BitConverter.SingleToInt32Bits(value)), 4);
        public void WriteF64(double value) =>
            WriteUnsigned(unchecked((ulong)BitConverter.DoubleToInt64Bits(value)), 8);

        public void WriteI128(RilsInt128 value)
        {
            EnsureCapacity(16);
            if (_offset != 0) throw new InvalidOperationException("A 128-bit field must occupy the full InlineValue payload.");
            _low = value.Low;
            _high = unchecked((ulong)value.High);
            _offset = 16;
        }

        public void WriteU128(RilsUInt128 value)
        {
            EnsureCapacity(16);
            if (_offset != 0) throw new InvalidOperationException("A 128-bit field must occupy the full InlineValue payload.");
            _low = value.Low;
            _high = value.High;
            _offset = 16;
        }

        public RilsInlineValue Build() => new RilsInlineValue(_low, _high);

        private void WriteUnsigned(ulong value, int byteCount)
        {
            EnsureCapacity(byteCount);
            ulong masked = byteCount == 8 ? value : value & ((1UL << byteCount * 8) - 1UL);
            if (_offset < 8)
            {
                int lowBytes = Math.Min(byteCount, 8 - _offset);
                _low |= masked << (_offset * 8);
                if (lowBytes < byteCount) _high |= masked >> (lowBytes * 8);
            }
            else
            {
                _high |= masked << ((_offset - 8) * 8);
            }
            _offset += byteCount;
        }

        private void EnsureCapacity(int byteCount)
        {
            if (byteCount < 0 || _offset > 16 - byteCount)
                throw new InvalidOperationException("InlineValue fields exceed the 16-byte ABI payload.");
        }
    }

    /// Allocation-free field reader for the canonical packed InlineValue ABI.
    public struct RilsInlineValueReader
    {
        private readonly ulong _low;
        private readonly ulong _high;
        private int _offset;

        public RilsInlineValueReader(RilsInlineValue value)
        {
            _low = value.Low;
            _high = value.High;
            _offset = 0;
        }

        public int BytesRead => _offset;
        public bool ReadBool()
        {
            ulong value = ReadUnsigned(1);
            if (value > 1) throw new InvalidOperationException("InlineValue bool fields must be encoded as 0 or 1.");
            return value != 0;
        }
        public sbyte ReadI8() => unchecked((sbyte)ReadUnsigned(1));
        public short ReadI16() => unchecked((short)ReadUnsigned(2));
        public int ReadI32() => unchecked((int)ReadUnsigned(4));
        public long ReadI64() => unchecked((long)ReadUnsigned(8));
        public byte ReadU8() => unchecked((byte)ReadUnsigned(1));
        public ushort ReadU16() => unchecked((ushort)ReadUnsigned(2));
        public uint ReadU32() => unchecked((uint)ReadUnsigned(4));
        public ulong ReadU64() => ReadUnsigned(8);
        public float ReadF32() => BitConverter.Int32BitsToSingle(ReadI32());
        public double ReadF64() => BitConverter.Int64BitsToDouble(ReadI64());

        public RilsInt128 ReadI128()
        {
            EnsureCapacity(16);
            if (_offset != 0) throw new InvalidOperationException("A 128-bit field must occupy the full InlineValue payload.");
            _offset = 16;
            return new RilsInt128(_low, unchecked((long)_high));
        }

        public RilsUInt128 ReadU128()
        {
            EnsureCapacity(16);
            if (_offset != 0) throw new InvalidOperationException("A 128-bit field must occupy the full InlineValue payload.");
            _offset = 16;
            return new RilsUInt128(_low, _high);
        }

        private ulong ReadUnsigned(int byteCount)
        {
            EnsureCapacity(byteCount);
            ulong value;
            if (_offset < 8)
            {
                int lowBytes = Math.Min(byteCount, 8 - _offset);
                value = _low >> (_offset * 8);
                if (lowBytes < byteCount) value |= _high << (lowBytes * 8);
            }
            else
            {
                value = _high >> ((_offset - 8) * 8);
            }
            _offset += byteCount;
            return byteCount == 8 ? value : value & ((1UL << byteCount * 8) - 1UL);
        }

        private void EnsureCapacity(int byteCount)
        {
            if (byteCount < 0 || _offset > 16 - byteCount)
                throw new InvalidOperationException("InlineValue fields exceed the 16-byte ABI payload.");
        }
    }

    public readonly struct RilsValue : IEquatable<RilsValue>
    {
        private readonly ulong _low;
        private readonly ulong _high;
        private readonly string? _string;

        private RilsValue(RilsValueTag tag, ulong low, ulong high = 0, string? managedString = null)
        {
            Tag = tag;
            _low = low;
            _high = high;
            _string = managedString;
        }

        public RilsValueTag Tag { get; }

        public static RilsValue Unit => new RilsValue(RilsValueTag.Unit, 0);
        public static RilsValue From(bool value) => new RilsValue(RilsValueTag.Bool, value ? 1UL : 0UL);
        public static RilsValue From(sbyte value) => new RilsValue(RilsValueTag.I8, unchecked((ulong)(long)value));
        public static RilsValue From(short value) => new RilsValue(RilsValueTag.I16, unchecked((ulong)(long)value));
        public static RilsValue From(int value) => new RilsValue(RilsValueTag.I32, unchecked((ulong)(long)value));
        public static RilsValue From(long value) => new RilsValue(RilsValueTag.I64, unchecked((ulong)value));
        public static RilsValue From(RilsInt128 value) => new RilsValue(RilsValueTag.I128, value.Low, unchecked((ulong)value.High));
        public static RilsValue FromIsize(IntPtr value) => new RilsValue(RilsValueTag.Isize, unchecked((ulong)value.ToInt64()));
        public static RilsValue From(byte value) => new RilsValue(RilsValueTag.U8, value);
        public static RilsValue From(ushort value) => new RilsValue(RilsValueTag.U16, value);
        public static RilsValue From(uint value) => new RilsValue(RilsValueTag.U32, value);
        public static RilsValue From(ulong value) => new RilsValue(RilsValueTag.U64, value);
        public static RilsValue From(RilsUInt128 value) => new RilsValue(RilsValueTag.U128, value.Low, value.High);
        public static RilsValue FromUsize(UIntPtr value) => new RilsValue(RilsValueTag.Usize, value.ToUInt64());
        public static RilsValue From(float value) => new RilsValue(RilsValueTag.F32, unchecked((uint)BitConverter.SingleToInt32Bits(value)));
        public static RilsValue From(double value) => new RilsValue(RilsValueTag.F64, unchecked((ulong)BitConverter.DoubleToInt64Bits(value)));
        public static RilsValue From(char value) => From(new RilsChar(value));
        public static RilsValue From(RilsChar value) => new RilsValue(RilsValueTag.Char, value.Value);
        public static RilsValue From(string value) => new RilsValue(
            RilsValueTag.String,
            0,
            0,
            value ?? throw new ArgumentNullException(nameof(value)));
        public static RilsValue From(RilsObjectHandle value) => new RilsValue(
            RilsValueTag.HostHandle,
            unchecked((ulong)value.ObjectId),
            (ulong)value.Generation << 32 | value.TypeId);
        public static RilsValue From(RilsInlineValue value) =>
            new RilsValue(RilsValueTag.InlineValue, value.Low, value.High);

        public bool AsBool() { Require(RilsValueTag.Bool); return _low != 0; }
        public sbyte AsI8() { Require(RilsValueTag.I8); return unchecked((sbyte)_low); }
        public short AsI16() { Require(RilsValueTag.I16); return unchecked((short)_low); }
        public int AsI32() { Require(RilsValueTag.I32); return unchecked((int)_low); }
        public long AsI64() { Require(RilsValueTag.I64); return unchecked((long)_low); }
        public RilsInt128 AsI128() { Require(RilsValueTag.I128); return new RilsInt128(_low, unchecked((long)_high)); }
        public IntPtr AsIsize() { Require(RilsValueTag.Isize); return new IntPtr(unchecked((long)_low)); }
        public byte AsU8() { Require(RilsValueTag.U8); return unchecked((byte)_low); }
        public ushort AsU16() { Require(RilsValueTag.U16); return unchecked((ushort)_low); }
        public uint AsU32() { Require(RilsValueTag.U32); return unchecked((uint)_low); }
        public ulong AsU64() { Require(RilsValueTag.U64); return _low; }
        public RilsUInt128 AsU128() { Require(RilsValueTag.U128); return new RilsUInt128(_low, _high); }
        public UIntPtr AsUsize() { Require(RilsValueTag.Usize); return new UIntPtr(_low); }
        public float AsF32() { Require(RilsValueTag.F32); return BitConverter.Int32BitsToSingle(unchecked((int)_low)); }
        public double AsF64() { Require(RilsValueTag.F64); return BitConverter.Int64BitsToDouble(unchecked((long)_low)); }
        public RilsChar AsChar() { Require(RilsValueTag.Char); return new RilsChar(checked((uint)_low)); }
        public string AsString() { Require(RilsValueTag.String); return _string!; }
        public RilsObjectHandle AsHostHandle(ulong sessionId)
        {
            Require(RilsValueTag.HostHandle);
            uint generation = checked((uint)(_high >> 32));
            uint typeId = checked((uint)(_high & uint.MaxValue));
            return new RilsObjectHandle(sessionId, unchecked((long)_low), generation, typeId);
        }
        public RilsInlineValue AsInlineValue()
        {
            Require(RilsValueTag.InlineValue);
            return new RilsInlineValue(_low, _high);
        }

        internal NativeValue ToNative() => Tag == RilsValueTag.String
            ? new NativeValue { Tag = Tag, Low = NativeInterop.CreateString(_string!), High = 0 }
            : new NativeValue { Tag = Tag, Low = _low, High = _high };
        internal static RilsValue FromNative(NativeValue value) => value.Tag == RilsValueTag.String
            ? From(NativeInterop.TakeString(value.Low))
            : new RilsValue(value.Tag, value.Low, value.High);

        private void Require(RilsValueTag expected)
        {
            if (Tag != expected)
            {
                throw new InvalidOperationException($"Rils value is {Tag}, not {expected}.");
            }
        }

        public bool Equals(RilsValue other) => Tag == other.Tag && _low == other._low &&
            _high == other._high && string.Equals(_string, other._string, StringComparison.Ordinal);
        public override bool Equals(object? obj) => obj is RilsValue other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Tag, _low, _high, _string);
        public static bool operator ==(RilsValue left, RilsValue right) => left.Equals(right);
        public static bool operator !=(RilsValue left, RilsValue right) => !left.Equals(right);

        public static implicit operator RilsValue(bool value) => From(value);
        public static implicit operator RilsValue(sbyte value) => From(value);
        public static implicit operator RilsValue(short value) => From(value);
        public static implicit operator RilsValue(int value) => From(value);
        public static implicit operator RilsValue(long value) => From(value);
        public static implicit operator RilsValue(byte value) => From(value);
        public static implicit operator RilsValue(ushort value) => From(value);
        public static implicit operator RilsValue(uint value) => From(value);
        public static implicit operator RilsValue(ulong value) => From(value);
        public static implicit operator RilsValue(float value) => From(value);
        public static implicit operator RilsValue(double value) => From(value);
        public static implicit operator RilsValue(char value) => From(value);
        public static implicit operator RilsValue(string value) => From(value);
    }
}
