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

    public readonly struct RilsValue : IEquatable<RilsValue>
    {
        private readonly ulong _low;
        private readonly ulong _high;

        private RilsValue(RilsValueTag tag, ulong low, ulong high = 0)
        {
            Tag = tag;
            _low = low;
            _high = high;
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

        internal NativeValue ToNative() => new NativeValue { Tag = Tag, Low = _low, High = _high };
        internal static RilsValue FromNative(NativeValue value) => new RilsValue(value.Tag, value.Low, value.High);

        private void Require(RilsValueTag expected)
        {
            if (Tag != expected)
            {
                throw new InvalidOperationException($"Rils value is {Tag}, not {expected}.");
            }
        }

        public bool Equals(RilsValue other) => Tag == other.Tag && _low == other._low && _high == other._high;
        public override bool Equals(object? obj) => obj is RilsValue other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(Tag, _low, _high);
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
    }
}
