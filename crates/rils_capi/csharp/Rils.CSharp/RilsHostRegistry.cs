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
            RilsHostFunctionDescriptor descriptor,
            Func<RilsValue[], RilsValue> handler)
        {
            Descriptor = descriptor ?? throw new ArgumentNullException(nameof(descriptor));
            Handler = handler ?? throw new ArgumentNullException(nameof(handler));
            ParameterTags = CreateTags(descriptor.Parameters);
        }

        public RilsHostFunction(
            ulong functionId,
            string name,
            string capability,
            RilsValueTag returnTag,
            IReadOnlyList<RilsValueTag> parameterTags,
            Func<RilsValue[], RilsValue> handler)
        {
            if (functionId == 0) throw new ArgumentOutOfRangeException(nameof(functionId));
            if (parameterTags == null) throw new ArgumentNullException(nameof(parameterTags));
            RilsHostParameter[] parameters = CreateParameters(parameterTags);
            Descriptor = new RilsHostFunctionDescriptor(
                functionId,
                name,
                capability,
                new RilsHostParameter(returnTag),
                parameters);
            ParameterTags = parameterTags;
            Handler = handler ?? throw new ArgumentNullException(nameof(handler));
        }

        public RilsHostFunction(
            ulong functionId,
            string name,
            string capability,
            RilsHostParameter returnParameter,
            IReadOnlyList<RilsHostParameter> parameters,
            Func<RilsValue[], RilsValue> handler,
            RilsHostThreadPolicy threadPolicy = RilsHostThreadPolicy.MainThreadOnly,
            RilsHostReceiver receiver = RilsHostReceiver.None)
        {
            if (functionId == 0) throw new ArgumentOutOfRangeException(nameof(functionId));
            Descriptor = new RilsHostFunctionDescriptor(
                functionId,
                name,
                capability,
                returnParameter,
                parameters,
                threadPolicy,
                receiver);
            ParameterTags = CreateTags(Descriptor.Parameters);
            Handler = handler ?? throw new ArgumentNullException(nameof(handler));
        }

        public RilsHostFunctionDescriptor Descriptor { get; }
        public ulong FunctionId => Descriptor.FunctionId;
        public string Name => Descriptor.Name;
        public string Capability => Descriptor.Capability;
        public RilsValueTag ReturnTag => Descriptor.ReturnParameter.Tag;
        public IReadOnlyList<RilsValueTag> ParameterTags { get; }
        public IReadOnlyList<RilsHostParameter> Parameters => Descriptor.Parameters;
        public RilsHostTransferMode ReturnTransferMode => Descriptor.ReturnParameter.TransferMode;
        public RilsHostThreadPolicy ThreadPolicy => Descriptor.ThreadPolicy;
        public RilsHostReceiver Receiver => Descriptor.Receiver;
        internal Func<RilsValue[], RilsValue> Handler { get; }

        private static RilsHostParameter[] CreateParameters(IReadOnlyList<RilsValueTag> tags)
        {
            var parameters = new RilsHostParameter[tags.Count];
            for (int index = 0; index < parameters.Length; index++)
            {
                parameters[index] = new RilsHostParameter(tags[index]);
            }
            return parameters;
        }

        private static IReadOnlyList<RilsValueTag> CreateTags(IReadOnlyList<RilsHostParameter> parameters)
        {
            var tags = new RilsValueTag[parameters.Count];
            for (int index = 0; index < tags.Length; index++) tags[index] = parameters[index].Tag;
            return tags;
        }
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
        private readonly int _ownerThreadId;

        public RilsHostRegistry(RilsRuntime runtime)
        {
            _runtime = runtime ?? throw new ArgumentNullException(nameof(runtime));
            _ownerThreadId = Environment.CurrentManagedThreadId;
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
            if (function.ThreadPolicy == RilsHostThreadPolicy.ScheduleToMainThread)
            {
                throw new NotSupportedException(
                    "Main-thread scheduling is not enabled by the synchronous host bridge yet.");
            }
            try
            {
                RilsHostDeclarationInterop.Register(_runtime, function.Descriptor);
            }
            catch (RilsException exception) when (
                exception.Message.IndexOf("already declared", StringComparison.Ordinal) >= 0)
            {
                // The declaration came from a registered manifest fragment;
                // retain the managed callback for dispatch.
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
            if (function.ThreadPolicy == RilsHostThreadPolicy.MainThreadOnly &&
                Environment.CurrentManagedThreadId != _ownerThreadId)
            {
                if (outError != null) *outError = default;
                return (int)RilsStatus.ExecutionError;
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

    internal static unsafe class RilsHostDeclarationInterop
    {
        internal static void Register(
            RilsRuntime runtime,
            RilsHostFunctionDescriptor function)
        {
            byte[] name = Encoding.UTF8.GetBytes(function.Name);
            byte[] capability = Encoding.UTF8.GetBytes(function.Capability);
            uint[] tags = new uint[function.Parameters.Count];
            for (int index = 0; index < tags.Length; index++)
            {
                tags[index] = (uint)function.Parameters[index].Tag;
            }

            fixed (byte* namePointer = name)
            fixed (byte* capabilityPointer = capability)
            fixed (uint* tagPointer = tags)
            {
                var native = new NativeHostFunction
                {
                    FunctionId = function.FunctionId,
                    Name = NativeInterop.Slice(namePointer, name.Length),
                    Capability = NativeInterop.Slice(capabilityPointer, capability.Length),
                    ParameterTags = tagPointer,
                    ParameterCount = new UIntPtr(checked((uint)tags.Length)),
                    ReturnTag = function.ReturnParameter.Tag,
                    Reserved = (uint)function.Receiver,
                };
                NativeInterop.Check(NativeMethods.RuntimeRegisterHostFunctions(
                    runtime.Handle,
                    &native,
                    new UIntPtr(1)));
            }
        }
    }
}
