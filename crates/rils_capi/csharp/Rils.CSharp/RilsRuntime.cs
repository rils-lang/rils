#nullable enable
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Rils.CSharp
{
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

        private static RilsException ReadError(RilsStatus status)
        {
            // Error getters do not clear or replace the thread-local native error.
            string message = ReadUtf8(NativeMethods.LastErrorMessage());
            string sourceName = ReadUtf8(NativeMethods.LastErrorSourceName());
            ulong spanStart = NativeMethods.LastErrorSpanStart();
            ulong spanEnd = NativeMethods.LastErrorSpanEnd();
            return new RilsException(status, message, sourceName, spanStart, spanEnd);
        }

        private static string ReadUtf8(NativeSlice slice)
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
        private ulong _handle;

        internal RilsInstance(RilsModule module, ulong handle)
        {
            _module = module;
            _handle = handle;
        }

        public bool IsDisposed => _handle == 0;

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
            NativeInterop.Check(NativeMethods.InstanceDestroy(_module.Runtime.Handle, _handle));
            _handle = 0;
            _module.Unregister(this);
        }

        private void EnsureUsable()
        {
            _module.EnsureUsable();
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(RilsInstance));
            }
        }
    }
}
