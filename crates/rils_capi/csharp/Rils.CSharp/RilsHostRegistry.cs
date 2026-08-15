#nullable enable
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace Rils.CSharp
{
    /// Describes one scalar host function exposed to Rils.
    public sealed class RilsHostFunction
    {
        public RilsHostFunction(
            ulong functionId,
            string name,
            string capability,
            RilsValueTag returnTag,
            IReadOnlyList<RilsValueTag> parameterTags,
            Func<RilsValue[], RilsValue> handler)
        {
            if (functionId == 0) throw new ArgumentOutOfRangeException(nameof(functionId));
            Name = name ?? throw new ArgumentNullException(nameof(name));
            Capability = capability ?? throw new ArgumentNullException(nameof(capability));
            ParameterTags = parameterTags ?? throw new ArgumentNullException(nameof(parameterTags));
            Handler = handler ?? throw new ArgumentNullException(nameof(handler));
            FunctionId = functionId;
            ReturnTag = returnTag;
        }

        public ulong FunctionId { get; }
        public string Name { get; }
        public string Capability { get; }
        public RilsValueTag ReturnTag { get; }
        public IReadOnlyList<RilsValueTag> ParameterTags { get; }
        internal Func<RilsValue[], RilsValue> Handler { get; }
    }

    /// Minimal synchronous scalar bridge for the Unity prototype.
    /// Registration must finish before the runtime is frozen or a module is created.
    public sealed unsafe class RilsHostRegistry : IDisposable
    {
        private static readonly NativeHostDispatcher Dispatcher = Dispatch;

        private readonly RilsRuntime _runtime;
        private readonly Dictionary<ulong, RilsHostFunction> _functions =
            new Dictionary<ulong, RilsHostFunction>();
        private readonly GCHandle _selfHandle;
        private bool _frozen;
        private bool _disposed;

        public RilsHostRegistry(RilsRuntime runtime)
        {
            _runtime = runtime ?? throw new ArgumentNullException(nameof(runtime));
            _selfHandle = GCHandle.Alloc(this, GCHandleType.Normal);
            NativeInterop.Check(NativeMethods.RuntimeSetHostDispatcher(
                _runtime.Handle,
                Dispatcher,
                GCHandle.ToIntPtr(_selfHandle)));
        }

        public void Register(RilsHostFunction function)
        {
            EnsureOpen();
            if (function == null) throw new ArgumentNullException(nameof(function));
            if (_frozen) throw new InvalidOperationException("The Rils host registry is already frozen.");
            if (_functions.ContainsKey(function.FunctionId))
            {
                throw new InvalidOperationException(
                    $"A host function with ID {function.FunctionId} is already registered.");
            }

            byte[] name = Encoding.UTF8.GetBytes(function.Name);
            byte[] capability = Encoding.UTF8.GetBytes(function.Capability);
            uint[] tags = new uint[function.ParameterTags.Count];
            for (int index = 0; index < tags.Length; index++)
            {
                tags[index] = (uint)function.ParameterTags[index];
            }

            fixed (byte* namePointer = name)
            fixed (byte* capabilityPointer = capability)
            fixed (uint* tagPointer = tags)
            {
                var descriptor = new NativeHostFunction
                {
                    FunctionId = function.FunctionId,
                    Name = NativeInterop.Slice(namePointer, name.Length),
                    Capability = NativeInterop.Slice(capabilityPointer, capability.Length),
                    ParameterTags = tagPointer,
                    ParameterCount = new UIntPtr(checked((uint)tags.Length)),
                    ReturnTag = function.ReturnTag,
                    Reserved = 0,
                };
                NativeInterop.Check(NativeMethods.RuntimeRegisterHostFunctions(
                    _runtime.Handle,
                    &descriptor,
                    new UIntPtr(1)));
            }
            _functions.Add(function.FunctionId, function);
        }

        public void AllowCapability(string capability)
        {
            EnsureOpen();
            if (capability == null) throw new ArgumentNullException(nameof(capability));
            byte[] bytes = Encoding.UTF8.GetBytes(capability);
            fixed (byte* pointer = bytes)
            {
                NativeInterop.Check(NativeMethods.RuntimeAllowCapability(
                    _runtime.Handle,
                    NativeInterop.Slice(pointer, bytes.Length)));
            }
        }

        public void Freeze()
        {
            EnsureOpen();
            NativeInterop.Check(NativeMethods.RuntimeFreezeHostRegistry(_runtime.Handle));
            _frozen = true;
        }

        public void Dispose()
        {
            if (_disposed) return;
            if (_frozen && !_runtime.IsDisposed)
            {
                throw new InvalidOperationException(
                    "A frozen host registry must be disposed after its RilsRuntime.");
            }
            if (!_runtime.IsDisposed && !_frozen)
            {
                NativeInterop.Check(NativeMethods.RuntimeSetHostDispatcher(
                    _runtime.Handle,
                    null!,
                    IntPtr.Zero));
            }
            _selfHandle.Free();
            _disposed = true;
        }

        private void EnsureOpen()
        {
            _runtime.EnsureUsable();
            if (_disposed) throw new ObjectDisposedException(nameof(RilsHostRegistry));
        }

        private static int Dispatch(
            IntPtr userData,
            ulong functionId,
            NativeValue* arguments,
            UIntPtr argumentCount,
            NativeValue* outValue,
            NativeSlice* outError)
        {
            if (userData == IntPtr.Zero || arguments == null || outValue == null)
            {
                return (int)RilsStatus.InvalidArgument;
            }
            var handle = GCHandle.FromIntPtr(userData);
            if (!(handle.Target is RilsHostRegistry registry))
            {
                return (int)RilsStatus.InvalidHandle;
            }
            return registry.Dispatch(functionId, arguments, argumentCount, outValue, outError);
        }

        private int Dispatch(
            ulong functionId,
            NativeValue* arguments,
            UIntPtr argumentCount,
            NativeValue* outValue,
            NativeSlice* outError)
        {
            if (!_functions.TryGetValue(functionId, out RilsHostFunction? function))
            {
                return (int)RilsStatus.InvalidArgument;
            }
            ulong count = argumentCount.ToUInt64();
            if (count > int.MaxValue || count != (ulong)function.ParameterTags.Count)
            {
                return (int)RilsStatus.InvalidArgument;
            }

            var values = new RilsValue[(int)count];
            for (int index = 0; index < values.Length; index++)
            {
                if (arguments[index].Tag != function.ParameterTags[index])
                {
                    return (int)RilsStatus.InvalidArgument;
                }
                values[index] = RilsValue.FromNative(arguments[index]);
            }

            try
            {
                RilsValue result = function.Handler(values);
                if (result.Tag != function.ReturnTag)
                {
                    return (int)RilsStatus.InvalidArgument;
                }
                *outValue = result.ToNative();
                return (int)RilsStatus.Ok;
            }
            catch
            {
                // The prototype deliberately returns a generic status. A later ABI revision
                // will add an owned error-buffer protocol for managed exception messages.
                if (outError != null) *outError = default;
                return (int)RilsStatus.ExecutionError;
            }
        }
    }
}
