#nullable enable
using System;

namespace Rils.CSharp
{
    /// Describes how a host boundary value crosses between Rils and C#.
    public enum RilsHostTransferMode
    {
        /// The value is copied without invoking a user-defined clone operation.
        Copy = 0,
        /// The value is copied through its explicit Clone contract.
        Clone = 1,
        /// The value is an opaque host handle and never owns the host object.
        Handle = 2,
    }

    /// Declares where a host function may execute.
    public enum RilsHostThreadPolicy
    {
        MainThreadOnly = 0,
        AnyThread = 1,
        ScheduleToMainThread = 2,
    }

    public enum RilsHostReceiver
    {
        None = 0,
        Self = 1,
        RefSelf = 2,
        RefMutSelf = 3,
    }

    /// Stable categories for errors produced by a host function.
    public enum RilsHostErrorCode
    {
        InvalidArgument = 1,
        InvalidHandle = 2,
        ObjectDestroyed = 3,
        WrongThread = 4,
        MissingCapability = 5,
        Unsupported = 6,
        UnityException = 7,
        Execution = 8,
    }

    /// Structured managed-side host error. The native ABI currently maps this to
    /// ExecutionError; the model is kept stable for the future error-buffer ABI.
    public sealed class RilsHostError
    {
        public RilsHostError(RilsHostErrorCode code, string message)
        {
            Code = code;
            Message = message ?? throw new ArgumentNullException(nameof(message));
        }

        public RilsHostErrorCode Code { get; }
        public string Message { get; }
        public string? FunctionName { get; internal set; }
    }

    /// Exception that a host handler can throw to preserve a structured error.
    public sealed class RilsHostException : Exception
    {
        public RilsHostException(RilsHostError error)
            : base(error?.Message ?? throw new ArgumentNullException(nameof(error)))
        {
            Error = error;
        }

        public RilsHostError Error { get; }
    }

    /// Opaque Unity object identity. It is session-bound and never a native pointer.
    public readonly struct RilsObjectHandle : IEquatable<RilsObjectHandle>
    {
        public RilsObjectHandle(ulong sessionId, long objectId, uint generation, uint typeId)
        {
            if (sessionId == 0) throw new ArgumentOutOfRangeException(nameof(sessionId));
            if (objectId == 0) throw new ArgumentOutOfRangeException(nameof(objectId));
            if (generation == 0) throw new ArgumentOutOfRangeException(nameof(generation));
            if (typeId == 0) throw new ArgumentOutOfRangeException(nameof(typeId));
            SessionId = sessionId;
            ObjectId = objectId;
            Generation = generation;
            TypeId = typeId;
        }

        public ulong SessionId { get; }
        public long ObjectId { get; }
        public uint Generation { get; }
        public uint TypeId { get; }

        public bool Equals(RilsObjectHandle other) =>
            SessionId == other.SessionId && ObjectId == other.ObjectId &&
            Generation == other.Generation && TypeId == other.TypeId;

        public override bool Equals(object? obj) => obj is RilsObjectHandle other && Equals(other);
        public override int GetHashCode() => HashCode.Combine(SessionId, ObjectId, Generation, TypeId);
        public static bool operator ==(RilsObjectHandle left, RilsObjectHandle right) => left.Equals(right);
        public static bool operator !=(RilsObjectHandle left, RilsObjectHandle right) => !left.Equals(right);
    }

    /// One parameter in a host function signature.
    public readonly struct RilsHostParameter
    {
        public RilsHostParameter(
            RilsValueTag tag,
            RilsHostTransferMode transferMode = RilsHostTransferMode.Copy,
            string? logicalTypeName = null)
        {
            if (logicalTypeName != null && !(
                tag == RilsValueTag.HostHandle && transferMode == RilsHostTransferMode.Handle ||
                tag == RilsValueTag.InlineValue && transferMode == RilsHostTransferMode.Copy))
            {
                throw new ArgumentException(
                    "Named host types must use handle or inline-value transport.",
                    nameof(logicalTypeName));
            }
            Tag = tag;
            TransferMode = transferMode;
            LogicalTypeName = logicalTypeName;
        }

        public RilsValueTag Tag { get; }
        public RilsHostTransferMode TransferMode { get; }
        /// The Manifest v4 logical type name. Named types use either HostHandle
        /// or the declared inline-value transport.
        public string? LogicalTypeName { get; }

        public static RilsHostParameter NamedHandle(string logicalTypeName)
        {
            if (string.IsNullOrWhiteSpace(logicalTypeName))
            {
                throw new ArgumentException("Logical host type name cannot be empty.", nameof(logicalTypeName));
            }
            return new RilsHostParameter(
                RilsValueTag.HostHandle,
                RilsHostTransferMode.Handle,
                logicalTypeName);
        }

        public static RilsHostParameter NamedValue(string logicalTypeName)
        {
            if (string.IsNullOrWhiteSpace(logicalTypeName))
            {
                throw new ArgumentException("Logical host type name cannot be empty.", nameof(logicalTypeName));
            }
            return new RilsHostParameter(
                RilsValueTag.InlineValue,
                RilsHostTransferMode.Copy,
                logicalTypeName);
        }
    }

    /// A value crossing from managed code into a Rils call together with its
    /// logical host type. Primitive arguments can use From; named objects use
    /// NamedHandle so the runtime can restore their nominal type and lineage.
    public readonly struct RilsHostArgument
    {
        public RilsHostArgument(RilsValue value, RilsHostParameter parameter)
        {
            if (value.Tag != parameter.Tag)
            {
                throw new ArgumentException(
                    "The host argument value and transport metadata must use the same tag.",
                    nameof(parameter));
            }
            Value = value;
            Parameter = parameter;
        }

        public RilsValue Value { get; }
        public RilsHostParameter Parameter { get; }

        public static RilsHostArgument From(RilsValue value)
        {
            RilsHostTransferMode transfer = value.Tag == RilsValueTag.HostHandle
                ? RilsHostTransferMode.Handle
                : RilsHostTransferMode.Copy;
            return new RilsHostArgument(value, new RilsHostParameter(value.Tag, transfer));
        }

        public static RilsHostArgument NamedHandle(RilsObjectHandle value, string logicalTypeName) =>
            new RilsHostArgument(
                RilsValue.From(value),
                RilsHostParameter.NamedHandle(logicalTypeName));

        public static RilsHostArgument NamedValue(RilsInlineValue value, string logicalTypeName) =>
            new RilsHostArgument(
                RilsValue.From(value),
                RilsHostParameter.NamedValue(logicalTypeName));
    }
}
