#nullable enable
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Rils.CSharp
{
    public readonly struct RilsHostFormatSpec
    {
        public RilsHostFormatSpec(RilsFormatKind kind, bool alternate = false, int? precision = null)
        {
            Kind = kind;
            Alternate = alternate;
            Precision = precision;
        }

        public RilsFormatKind Kind { get; }
        public bool Alternate { get; }
        public int? Precision { get; }
    }

    internal static unsafe class NativeInterop
    {
        internal static void Check(int status)
        {
            if (status == (int)RilsStatus.Ok)
            {
                return;
            }

            throw ReadError((RilsStatus)status);
        }

        internal static RilsException LastError()
        {
            return ReadError((RilsStatus)NativeMethods.LastErrorCode());
        }

        internal static NativeSlice Slice(byte* data, int length)
        {
            return new NativeSlice { Data = data, Length = new UIntPtr(checked((uint)length)) };
        }

        internal static ulong CreateString(string value)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(value);
            fixed (byte* pointer = bytes)
            {
                Check(NativeMethods.StringCreate(Slice(pointer, bytes.Length), out ulong handle));
                return handle;
            }
        }

        internal static string TakeString(ulong handle)
        {
            try
            {
                Check(NativeMethods.StringSize(handle, out UIntPtr nativeSize));
                ulong size = nativeSize.ToUInt64();
                if (size > int.MaxValue)
                    throw new InvalidOperationException("Native Rils string exceeds the managed array limit.");
                byte[] bytes = new byte[checked((int)size)];
                fixed (byte* pointer = bytes)
                {
                    Check(NativeMethods.StringWrite(
                        handle,
                        pointer,
                        new UIntPtr(checked((uint)bytes.Length)),
                        out UIntPtr written));
                    if (written.ToUInt64() != size)
                        throw new InvalidOperationException("Native Rils string length changed while copying.");
                }
                return Encoding.UTF8.GetString(bytes);
            }
            finally
            {
                Check(NativeMethods.StringDestroy(handle));
            }
        }

        private static RilsException ReadError(RilsStatus status)
        {
            // Error getters do not clear or replace the thread-local native error.
            string message = ReadUtf8(NativeMethods.LastErrorMessage());
            string sourceName = ReadUtf8(NativeMethods.LastErrorSourceName());
            ulong spanStart = NativeMethods.LastErrorSpanStart();
            ulong spanEnd = NativeMethods.LastErrorSpanEnd();
            return new RilsException(status, message, sourceName, spanStart, spanEnd);
        }

        internal static string ReadUtf8(NativeSlice slice)
        {
            ulong length = slice.Length.ToUInt64();
            if (length == 0)
            {
                return string.Empty;
            }
            if (slice.Data == null || length > int.MaxValue)
            {
                throw new InvalidOperationException("Native Rils error returned an invalid UTF-8 slice.");
            }
            byte[] bytes = new byte[checked((int)length)];
            Marshal.Copy((IntPtr)slice.Data, bytes, 0, bytes.Length);
            return Encoding.UTF8.GetString(bytes);
        }
    }

    public sealed class RilsRuntime : IDisposable
    {
        private readonly int _ownerThreadId;
        private readonly List<RilsModule> _modules = new List<RilsModule>();
        private NativeOutputCallback? _nativeOutputCallback;
        private Action<string, bool>? _outputHandler;
        private NativeHostValueFormatCallback? _nativeHostValueFormatter;
        private Func<string, RilsValue, RilsHostFormatSpec, string?>? _hostValueFormatter;
        private ulong _handle;

        public RilsRuntime()
        {
            _ownerThreadId = Environment.CurrentManagedThreadId;
            _handle = NativeMethods.RuntimeCreate();
            if (_handle == 0)
            {
                throw NativeInterop.LastError();
            }
        }

        public static uint NativeAbiVersion => NativeMethods.AbiVersion();

        public bool IsDisposed => _handle == 0;

        public void SetMaxSteps(ulong maxSteps)
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.RuntimeSetMaxSteps(_handle, maxSteps));
        }

        /// Routes formatted print output to a managed callback. Pass null to restore stdout.
        /// The callback runs synchronously on the thread executing Rils and must not throw.
        public void SetOutputHandler(Action<string, bool>? handler)
        {
            EnsureUsable();
            _outputHandler = handler;
            _nativeOutputCallback = handler == null ? null : DispatchOutput;
            NativeInterop.Check(NativeMethods.RuntimeSetOutputCallback(
                _handle,
                _nativeOutputCallback,
                IntPtr.Zero));
        }

        private void DispatchOutput(IntPtr userData, NativeSlice text, uint newline)
        {
            try
            {
                _outputHandler?.Invoke(NativeInterop.ReadUtf8(text), newline != 0);
            }
            catch (Exception exception)
            {
                System.Diagnostics.Debug.WriteLine(
                    $"Rils output handler threw an exception: {exception}");
            }
        }

        /// Routes portable host values through a managed formatter before print output is emitted.
        /// Returning null keeps the runtime's fallback representation for an unknown logical type.
        public unsafe void SetHostValueFormatter(
            Func<string, RilsValue, RilsHostFormatSpec, string?>? formatter)
        {
            EnsureUsable();
            _hostValueFormatter = formatter;
            _nativeHostValueFormatter = formatter == null ? null : DispatchHostValueFormat;
            NativeInterop.Check(NativeMethods.RuntimeSetHostValueFormatter(
                _handle,
                _nativeHostValueFormatter,
                IntPtr.Zero));
        }

        private unsafe UIntPtr DispatchHostValueFormat(
            IntPtr userData,
            NativeSlice logicalType,
            NativeValue value,
            uint kind,
            uint alternate,
            UIntPtr precision,
            byte* buffer,
            UIntPtr capacity)
        {
            try
            {
                ulong rawPrecision = precision.ToUInt64();
                ulong absentPrecision = UIntPtr.Size == 8 ? ulong.MaxValue : uint.MaxValue;
                int? managedPrecision = rawPrecision == absentPrecision
                    ? null
                    : checked((int)rawPrecision);
                string? formatted = _hostValueFormatter?.Invoke(
                    NativeInterop.ReadUtf8(logicalType),
                    RilsValue.FromNative(value),
                    new RilsHostFormatSpec(
                        (RilsFormatKind)kind,
                        alternate != 0,
                        managedPrecision));
                if (formatted == null) return UIntPtr.Size == 8
                    ? new UIntPtr(ulong.MaxValue)
                    : new UIntPtr(uint.MaxValue);
                byte[] bytes = Encoding.UTF8.GetBytes(formatted);
                if (buffer != null && capacity.ToUInt64() >= (ulong)bytes.Length)
                {
                    fixed (byte* source = bytes)
                    {
                        Buffer.MemoryCopy(
                            source,
                            buffer,
                            checked((long)capacity.ToUInt64()),
                            bytes.Length);
                    }
                }
                return new UIntPtr(checked((uint)bytes.Length));
            }
            catch (Exception exception)
            {
                System.Diagnostics.Debug.WriteLine(
                    $"Rils host value formatter threw an exception: {exception}");
                return UIntPtr.Size == 8
                    ? new UIntPtr(ulong.MaxValue)
                    : new UIntPtr(uint.MaxValue);
            }
        }

        public unsafe void RegisterHostManifest(byte[] manifest)
        {
            if (manifest == null) throw new ArgumentNullException(nameof(manifest));
            EnsureUsable();

            fixed (byte* manifestPointer = manifest)
            {
                NativeInterop.Check(NativeMethods.RuntimeRegisterHostManifest(
                    _handle,
                    NativeInterop.Slice(manifestPointer, manifest.Length)));
            }
        }

        public unsafe byte[] GetHostManifest()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.RuntimeHostManifestSize(_handle, out UIntPtr nativeSize));
            ulong size = nativeSize.ToUInt64();
            if (size > int.MaxValue)
            {
                throw new InvalidOperationException("Serialized Rils host manifest exceeds the managed array limit.");
            }

            byte[] manifest = new byte[checked((int)size)];
            fixed (byte* manifestPointer = manifest)
            {
                NativeInterop.Check(NativeMethods.RuntimeWriteHostManifest(
                    _handle,
                    manifestPointer,
                    new UIntPtr(checked((uint)manifest.Length)),
                    out UIntPtr nativeWritten));
                if (nativeWritten.ToUInt64() != size)
                {
                    throw new InvalidOperationException("Native Rils host manifest size changed while serializing.");
                }
            }
            return manifest;
        }

        /// Freezes the host declaration contract without installing managed callbacks.
        public void FreezeHostRegistry()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.RuntimeFreezeHostRegistry(_handle));
        }

        public unsafe void AllowCapability(string capability)
        {
            if (capability == null) throw new ArgumentNullException(nameof(capability));
            EnsureUsable();
            byte[] bytes = Encoding.UTF8.GetBytes(capability);
            fixed (byte* pointer = bytes)
            {
                NativeInterop.Check(NativeMethods.RuntimeAllowCapability(
                    _handle,
                    NativeInterop.Slice(pointer, bytes.Length)));
            }
        }

        /// Enables every host-backed capability provided by the Rils standard library.
        public void AllowStandardLibrary()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.RuntimeAllowStandardLibrary(_handle));
        }

        public unsafe RilsModule Compile(string source, string sourceName = "<memory>")
        {
            if (source == null) throw new ArgumentNullException(nameof(source));
            if (sourceName == null) throw new ArgumentNullException(nameof(sourceName));
            EnsureUsable();

            byte[] sourceBytes = Encoding.UTF8.GetBytes(source);
            byte[] nameBytes = Encoding.UTF8.GetBytes(sourceName);
            fixed (byte* sourcePointer = sourceBytes)
            fixed (byte* namePointer = nameBytes)
            {
                NativeInterop.Check(NativeMethods.ModuleCompile(
                    _handle,
                    NativeInterop.Slice(namePointer, nameBytes.Length),
                    NativeInterop.Slice(sourcePointer, sourceBytes.Length),
                    out ulong module));
                var result = new RilsModule(this, module);
                _modules.Add(result);
                return result;
            }
        }

        public unsafe RilsModule CompileFile(string path)
        {
            if (path == null) throw new ArgumentNullException(nameof(path));
            EnsureUsable();

            string fullPath = Path.GetFullPath(path);
            byte[] pathBytes = Encoding.UTF8.GetBytes(fullPath);
            fixed (byte* pathPointer = pathBytes)
            {
                NativeInterop.Check(NativeMethods.ModuleCompileFile(
                    _handle,
                    NativeInterop.Slice(pathPointer, pathBytes.Length),
                    out ulong module));
                var result = new RilsModule(this, module);
                _modules.Add(result);
                return result;
            }
        }

        public unsafe RilsModule LoadBytecode(byte[] bytecode)
        {
            if (bytecode == null) throw new ArgumentNullException(nameof(bytecode));
            EnsureUsable();

            fixed (byte* bytecodePointer = bytecode)
            {
                NativeInterop.Check(NativeMethods.ModuleLoadBytecode(
                    _handle,
                    NativeInterop.Slice(bytecodePointer, bytecode.Length),
                    out ulong module));
                var result = new RilsModule(this, module);
                _modules.Add(result);
                return result;
            }
        }

        public unsafe RilsModule LoadBytecodeFile(string path)
        {
            if (path == null) throw new ArgumentNullException(nameof(path));
            EnsureUsable();

            string fullPath = Path.GetFullPath(path);
            byte[] pathBytes = Encoding.UTF8.GetBytes(fullPath);
            fixed (byte* pathPointer = pathBytes)
            {
                NativeInterop.Check(NativeMethods.ModuleLoadBytecodeFile(
                    _handle,
                    NativeInterop.Slice(pathPointer, pathBytes.Length),
                    out ulong module));
                var result = new RilsModule(this, module);
                _modules.Add(result);
                return result;
            }
        }

        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }
            EnsureOwnerThread();
            foreach (RilsModule module in _modules.ToArray())
            {
                module.Dispose();
            }
            NativeInterop.Check(NativeMethods.RuntimeDestroy(_handle));
            _handle = 0;
            _nativeOutputCallback = null;
            _outputHandler = null;
            _nativeHostValueFormatter = null;
            _hostValueFormatter = null;
        }

        internal ulong Handle
        {
            get { EnsureUsable(); return _handle; }
        }

        internal void EnsureUsable()
        {
            EnsureOwnerThread();
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(RilsRuntime));
            }
        }

        internal void Unregister(RilsModule module) => _modules.Remove(module);

        private void EnsureOwnerThread()
        {
            if (Environment.CurrentManagedThreadId != _ownerThreadId)
            {
                throw new InvalidOperationException("Rils handles must be used and disposed on their creating thread.");
            }
        }
    }

    public sealed class RilsModule : IDisposable
    {
        private readonly RilsRuntime _runtime;
        private readonly List<RilsInstance> _instances = new List<RilsInstance>();
        private ulong _handle;

        internal RilsModule(RilsRuntime runtime, ulong handle)
        {
            _runtime = runtime;
            _handle = handle;
        }

        public bool IsDisposed => _handle == 0;

        public RilsInstance CreateInstance()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.InstanceCreate(_runtime.Handle, _handle, out ulong instance));
            var result = new RilsInstance(this, instance);
            _instances.Add(result);
            return result;
        }

        public void ValidateHost()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.ModuleValidateHost(_runtime.Handle, _handle));
        }

        public unsafe IReadOnlyList<string> GetTraitImplementations(
            string traitName,
            string? sourceName = null)
        {
            if (traitName == null) throw new ArgumentNullException(nameof(traitName));
            EnsureUsable();
            byte[] traitBytes = Encoding.UTF8.GetBytes(traitName);
            byte[] sourceBytes = Encoding.UTF8.GetBytes(sourceName ?? string.Empty);
            fixed (byte* traitPointer = traitBytes)
            fixed (byte* sourcePointer = sourceBytes)
            {
                NativeSlice traitSlice = NativeInterop.Slice(traitPointer, traitBytes.Length);
                NativeSlice sourceSlice = NativeInterop.Slice(sourcePointer, sourceBytes.Length);
                NativeInterop.Check(NativeMethods.ModuleTraitImplementationCount(
                    _runtime.Handle,
                    _handle,
                    traitSlice,
                    sourceSlice,
                    out UIntPtr nativeCount));
                ulong count = nativeCount.ToUInt64();
                if (count > int.MaxValue)
                {
                    throw new InvalidOperationException("Rils trait implementation count exceeds the managed list limit.");
                }

                var implementations = new List<string>(checked((int)count));
                for (int index = 0; index < (int)count; index++)
                {
                    UIntPtr nativeIndex = new UIntPtr(checked((uint)index));
                    NativeInterop.Check(NativeMethods.ModuleTraitImplementationNameSize(
                        _runtime.Handle,
                        _handle,
                        traitSlice,
                        sourceSlice,
                        nativeIndex,
                        out UIntPtr nativeSize));
                    ulong size = nativeSize.ToUInt64();
                    if (size > int.MaxValue)
                    {
                        throw new InvalidOperationException("Rils trait implementation name exceeds the managed string limit.");
                    }
                    byte[] nameBytes = new byte[checked((int)size)];
                    fixed (byte* namePointer = nameBytes)
                    {
                        NativeInterop.Check(NativeMethods.ModuleWriteTraitImplementationName(
                            _runtime.Handle,
                            _handle,
                            traitSlice,
                            sourceSlice,
                            nativeIndex,
                            namePointer,
                            new UIntPtr(checked((uint)nameBytes.Length)),
                            out UIntPtr nativeWritten));
                        if (nativeWritten.ToUInt64() != size)
                        {
                            throw new InvalidOperationException("Native Rils trait implementation name changed while reading it.");
                        }
                    }
                    implementations.Add(Encoding.UTF8.GetString(nameBytes));
                }
                return implementations;
            }
        }

        public unsafe byte[] GetBytecode()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.ModuleBytecodeSize(
                _runtime.Handle,
                _handle,
                out UIntPtr nativeSize));
            ulong size = nativeSize.ToUInt64();
            if (size > int.MaxValue)
            {
                throw new InvalidOperationException("Serialized Rils bytecode exceeds the managed array limit.");
            }

            byte[] bytecode = new byte[checked((int)size)];
            fixed (byte* bytecodePointer = bytecode)
            {
                NativeInterop.Check(NativeMethods.ModuleWriteBytecode(
                    _runtime.Handle,
                    _handle,
                    bytecodePointer,
                    new UIntPtr(checked((uint)bytecode.Length)),
                    out UIntPtr nativeWritten));
                if (nativeWritten.ToUInt64() != size)
                {
                    throw new InvalidOperationException("Native Rils bytecode size changed while serializing.");
                }
            }
            return bytecode;
        }

        public unsafe void WriteBytecodeFile(string path)
        {
            if (path == null) throw new ArgumentNullException(nameof(path));
            EnsureUsable();

            string fullPath = Path.GetFullPath(path);
            byte[] pathBytes = Encoding.UTF8.GetBytes(fullPath);
            fixed (byte* pathPointer = pathBytes)
            {
                NativeInterop.Check(NativeMethods.ModuleWriteBytecodeFile(
                    _runtime.Handle,
                    _handle,
                    NativeInterop.Slice(pathPointer, pathBytes.Length)));
            }
        }

        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }
            _runtime.EnsureUsable();
            foreach (RilsInstance instance in _instances.ToArray())
            {
                instance.Dispose();
            }
            NativeInterop.Check(NativeMethods.ModuleDestroy(_runtime.Handle, _handle));
            _handle = 0;
            _runtime.Unregister(this);
        }

        internal RilsRuntime Runtime => _runtime;

        internal ulong Handle
        {
            get { EnsureUsable(); return _handle; }
        }

        internal void EnsureUsable()
        {
            _runtime.EnsureUsable();
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(RilsModule));
            }
        }

        internal void Unregister(RilsInstance instance) => _instances.Remove(instance);
    }

    public sealed class RilsInstance : IDisposable
    {
        private readonly RilsModule _module;
        private readonly List<RilsScriptValue> _scriptValues = new List<RilsScriptValue>();
        private ulong _handle;

        internal RilsInstance(RilsModule module, ulong handle)
        {
            _module = module;
            _handle = handle;
        }

        public bool IsDisposed => _handle == 0;

        public unsafe RilsScriptValue CreateDefaultValue(string targetType)
        {
            if (targetType == null) throw new ArgumentNullException(nameof(targetType));
            EnsureUsable();
            byte[] targetBytes = Encoding.UTF8.GetBytes(targetType);
            fixed (byte* targetPointer = targetBytes)
            {
                NativeInterop.Check(NativeMethods.ScriptValueCreateDefault(
                    _module.Runtime.Handle,
                    _handle,
                    NativeInterop.Slice(targetPointer, targetBytes.Length),
                    out ulong value));
                var result = new RilsScriptValue(this, value);
                _scriptValues.Add(result);
                return result;
            }
        }

        public unsafe RilsValue Call(string functionName, params RilsValue[] arguments)
        {
            if (functionName == null) throw new ArgumentNullException(nameof(functionName));
            if (arguments == null) throw new ArgumentNullException(nameof(arguments));
            EnsureUsable();

            byte[] nameBytes = Encoding.UTF8.GetBytes(functionName);
            var nativeArguments = new NativeValue[arguments.Length];
            for (int index = 0; index < arguments.Length; index++)
            {
                nativeArguments[index] = arguments[index].ToNative();
            }

            fixed (byte* namePointer = nameBytes)
            fixed (NativeValue* argumentPointer = nativeArguments)
            {
                NativeInterop.Check(NativeMethods.InstanceCall(
                    _module.Runtime.Handle,
                    _handle,
                    NativeInterop.Slice(namePointer, nameBytes.Length),
                    argumentPointer,
                    new UIntPtr(checked((uint)nativeArguments.Length)),
                    out NativeValue result));
                return RilsValue.FromNative(result);
            }
        }

        public RilsValue Execute()
        {
            EnsureUsable();
            NativeInterop.Check(NativeMethods.InstanceExecute(
                _module.Runtime.Handle,
                _handle,
                out NativeValue result));
            return RilsValue.FromNative(result);
        }

        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }
            _module.EnsureUsable();
            foreach (RilsScriptValue value in _scriptValues.ToArray())
            {
                value.Dispose();
            }
            NativeInterop.Check(NativeMethods.InstanceDestroy(_module.Runtime.Handle, _handle));
            _handle = 0;
            _module.Unregister(this);
        }

        internal RilsModule Module => _module;

        internal ulong Handle
        {
            get { EnsureUsable(); return _handle; }
        }

        internal void EnsureUsable()
        {
            _module.EnsureUsable();
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(RilsInstance));
            }
        }

        internal void Unregister(RilsScriptValue value) => _scriptValues.Remove(value);
    }

    public sealed class RilsScriptValue : IDisposable
    {
        private readonly RilsInstance _instance;
        private ulong _handle;

        internal RilsScriptValue(RilsInstance instance, ulong handle)
        {
            _instance = instance;
            _handle = handle;
        }

        public bool IsDisposed => _handle == 0;

        public unsafe RilsValue CallTrait(
            string traitName,
            string methodName,
            params RilsValue[] arguments)
        {
            if (arguments == null) throw new ArgumentNullException(nameof(arguments));
            var typedArguments = new RilsHostArgument[arguments.Length];
            for (int index = 0; index < arguments.Length; index++)
            {
                typedArguments[index] = RilsHostArgument.From(arguments[index]);
            }
            return CallTraitTyped(traitName, methodName, typedArguments);
        }

        public unsafe RilsValue CallTraitTyped(
            string traitName,
            string methodName,
            params RilsHostArgument[] arguments)
        {
            if (traitName == null) throw new ArgumentNullException(nameof(traitName));
            if (methodName == null) throw new ArgumentNullException(nameof(methodName));
            if (arguments == null) throw new ArgumentNullException(nameof(arguments));
            EnsureUsable();

            byte[] traitBytes = Encoding.UTF8.GetBytes(traitName);
            byte[] methodBytes = Encoding.UTF8.GetBytes(methodName);
            var nativeArguments = new NativeValue[arguments.Length];
            var nativeTypes = new NativeHostParameter[arguments.Length];
            var logicalTypes = new byte[arguments.Length][];
            var pins = new GCHandle[arguments.Length];
            for (int index = 0; index < arguments.Length; index++)
            {
                nativeArguments[index] = arguments[index].Value.ToNative();
                string? logicalType = arguments[index].Parameter.LogicalTypeName;
                logicalTypes[index] = logicalType == null
                    ? Array.Empty<byte>()
                    : Encoding.UTF8.GetBytes(logicalType);
                byte* logicalPointer = null;
                if (logicalTypes[index].Length != 0)
                {
                    pins[index] = GCHandle.Alloc(logicalTypes[index], GCHandleType.Pinned);
                    logicalPointer = (byte*)pins[index].AddrOfPinnedObject();
                }
                nativeTypes[index] = new NativeHostParameter
                {
                    LogicalType = NativeInterop.Slice(logicalPointer, logicalTypes[index].Length),
                    TransportTag = arguments[index].Parameter.Tag,
                };
            }
            try
            {
                fixed (byte* traitPointer = traitBytes)
                fixed (byte* methodPointer = methodBytes)
                fixed (NativeValue* argumentPointer = nativeArguments)
                fixed (NativeHostParameter* typePointer = nativeTypes)
                {
                    NativeInterop.Check(NativeMethods.ScriptValueCallTrait(
                        _instance.Module.Runtime.Handle,
                        _instance.Handle,
                        _handle,
                        NativeInterop.Slice(traitPointer, traitBytes.Length),
                        NativeInterop.Slice(methodPointer, methodBytes.Length),
                        argumentPointer,
                        typePointer,
                        new UIntPtr(checked((uint)nativeArguments.Length)),
                        out NativeValue result));
                    return RilsValue.FromNative(result);
                }
            }
            finally
            {
                for (int index = 0; index < pins.Length; index++)
                {
                    if (pins[index].IsAllocated) pins[index].Free();
                }
            }
        }

        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }
            _instance.EnsureUsable();
            NativeInterop.Check(NativeMethods.ScriptValueDestroy(
                _instance.Module.Runtime.Handle,
                _handle));
            _handle = 0;
            _instance.Unregister(this);
        }

        private void EnsureUsable()
        {
            _instance.EnsureUsable();
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(RilsScriptValue));
            }
        }
    }
}
