#nullable enable
using System;
using System.Collections.Generic;
using System.Text;

namespace Rils.CSharp
{
    public sealed class RilsHostTypeDescriptor
    {
        public RilsHostTypeDescriptor(
            string name,
            string? baseTypeName = null,
            RilsValueTag transportTag = RilsValueTag.HostHandle)
        {
            if (string.IsNullOrWhiteSpace(name))
                throw new ArgumentException("Host type name cannot be empty.", nameof(name));
            if (baseTypeName != null && string.IsNullOrWhiteSpace(baseTypeName))
                throw new ArgumentException("Host base type name cannot be empty.", nameof(baseTypeName));
            if (string.Equals(name, baseTypeName, StringComparison.Ordinal))
                throw new ArgumentException("A host type cannot inherit itself.", nameof(baseTypeName));
            if (transportTag != RilsValueTag.HostHandle)
                throw new NotSupportedException("Manifest v2 currently supports HostHandle transport for named host types.");
            Name = name;
            BaseTypeName = baseTypeName;
            TransportTag = transportTag;
        }

        public string Name { get; }
        public string? BaseTypeName { get; }
        public RilsValueTag TransportTag { get; }
    }

    /// Describes one host function independently from its managed implementation.
    public sealed class RilsHostFunctionDescriptor
    {
        public RilsHostFunctionDescriptor(
            ulong functionId,
            string name,
            string capability,
            RilsHostParameter returnParameter,
            IReadOnlyList<RilsHostParameter> parameters,
            RilsHostThreadPolicy threadPolicy = RilsHostThreadPolicy.MainThreadOnly,
            RilsHostReceiver receiver = RilsHostReceiver.None,
            string? managedMemberName = null)
        {
            if (functionId == 0) throw new ArgumentOutOfRangeException(nameof(functionId));
            if (string.IsNullOrWhiteSpace(name)) throw new ArgumentException("Host function name cannot be empty.", nameof(name));
            if (string.IsNullOrWhiteSpace(capability)) throw new ArgumentException("Host capability cannot be empty.", nameof(capability));
            FunctionId = functionId;
            Name = name;
            Capability = capability;
            ReturnParameter = returnParameter;
            if (parameters == null) throw new ArgumentNullException(nameof(parameters));
            var parameterSnapshot = new RilsHostParameter[parameters.Count];
            for (int index = 0; index < parameters.Count; index++)
            {
                parameterSnapshot[index] = parameters[index];
            }
            Parameters = Array.AsReadOnly(parameterSnapshot);
            ThreadPolicy = threadPolicy;
            Receiver = receiver;
            ManagedMemberName = managedMemberName;
        }

        public ulong FunctionId { get; }
        public string Name { get; }
        public string Capability { get; }
        public RilsHostParameter ReturnParameter { get; }
        public IReadOnlyList<RilsHostParameter> Parameters { get; }
        public RilsHostThreadPolicy ThreadPolicy { get; }
        public RilsHostReceiver Receiver { get; }
        public string? ManagedMemberName { get; }
    }

    /// A deterministic manifest fragment boundary. Version 1 is the only module
    /// version currently representable through the native registration API.
    public sealed class RilsHostModuleDescriptor
    {
        public RilsHostModuleDescriptor(
            string name,
            uint version,
            IReadOnlyList<RilsHostFunctionDescriptor> functions)
            : this(name, version, Array.Empty<RilsHostTypeDescriptor>(), functions)
        {
        }

        public RilsHostModuleDescriptor(
            string name,
            uint version,
            IReadOnlyList<RilsHostTypeDescriptor> types,
            IReadOnlyList<RilsHostFunctionDescriptor> functions)
        {
            if (string.IsNullOrWhiteSpace(name)) throw new ArgumentException("Host module name cannot be empty.", nameof(name));
            if (version == 0) throw new ArgumentOutOfRangeException(nameof(version));
            Name = name;
            Version = version;
            if (types == null) throw new ArgumentNullException(nameof(types));
            var typeSnapshot = new RilsHostTypeDescriptor[types.Count];
            var typeNames = new HashSet<string>(StringComparer.Ordinal);
            for (int index = 0; index < types.Count; index++)
            {
                RilsHostTypeDescriptor type = types[index]
                    ?? throw new ArgumentException("Host module types cannot contain null.", nameof(types));
                if (!typeNames.Add(type.Name))
                    throw new ArgumentException($"Host type '{type.Name}' is duplicated.", nameof(types));
                typeSnapshot[index] = type;
            }
            for (int index = 0; index < typeSnapshot.Length; index++)
            {
                string? baseType = typeSnapshot[index].BaseTypeName;
                if (baseType != null && !typeNames.Contains(baseType))
                    throw new ArgumentException(
                        $"Host type '{typeSnapshot[index].Name}' inherits undeclared type '{baseType}'.",
                        nameof(types));
            }
            Types = Array.AsReadOnly(typeSnapshot);
            if (functions == null) throw new ArgumentNullException(nameof(functions));
            var functionSnapshot = new RilsHostFunctionDescriptor[functions.Count];

            var ids = new HashSet<ulong>();
            var names = new HashSet<string>(StringComparer.Ordinal);
            string prefix = name + "::";
            for (int index = 0; index < functions.Count; index++)
            {
                RilsHostFunctionDescriptor function = functions[index]
                    ?? throw new ArgumentException("Host module functions cannot contain null.", nameof(functions));
                functionSnapshot[index] = function;
                int separator = function.Name.LastIndexOf("::", StringComparison.Ordinal);
                if (separator <= 0 ||
                    !string.Equals(function.Name.Substring(0, separator + 2), prefix, StringComparison.Ordinal))
                {
                    throw new ArgumentException(
                        $"Host function '{function.Name}' does not belong directly to module '{name}'.",
                        nameof(functions));
                }
                if (!ids.Add(function.FunctionId))
                {
                    throw new ArgumentException($"Host function ID {function.FunctionId} is duplicated in module '{name}'.", nameof(functions));
                }
                if (!names.Add(function.Name))
                {
                    throw new ArgumentException($"Host function '{function.Name}' is duplicated in module '{name}'.", nameof(functions));
                }
                ValidateLogicalType(function.ReturnParameter, typeNames, function.Name);
                for (int parameterIndex = 0; parameterIndex < function.Parameters.Count; parameterIndex++)
                {
                    ValidateLogicalType(function.Parameters[parameterIndex], typeNames, function.Name);
                }
            }
            Functions = Array.AsReadOnly(functionSnapshot);
        }

        private static void ValidateLogicalType(
            RilsHostParameter parameter,
            HashSet<string> typeNames,
            string functionName)
        {
            if (parameter.LogicalTypeName != null && !typeNames.Contains(parameter.LogicalTypeName))
            {
                throw new ArgumentException(
                    $"Host function '{functionName}' references undeclared logical type '{parameter.LogicalTypeName}'.",
                    "functions");
            }
        }

        public string Name { get; }
        public uint Version { get; }
        public IReadOnlyList<RilsHostTypeDescriptor> Types { get; }
        public IReadOnlyList<RilsHostFunctionDescriptor> Functions { get; }
    }

    public static class RilsHostStableId
    {
        /// Produces a deterministic FNV-1a ID from a canonical managed member identity.
        public static ulong FromCanonicalName(string canonicalName)
        {
            if (string.IsNullOrWhiteSpace(canonicalName))
            {
                throw new ArgumentException("Canonical host member name cannot be empty.", nameof(canonicalName));
            }
            const ulong offset = 14695981039346656037UL;
            const ulong prime = 1099511628211UL;
            ulong hash = offset;
            byte[] bytes = Encoding.UTF8.GetBytes(canonicalName);
            for (int index = 0; index < bytes.Length; index++)
            {
                hash ^= bytes[index];
                hash *= prime;
            }
            return hash == 0 ? 1UL : hash;
        }
    }

    public static class RilsHostManifestBuilder
    {
        /// Builds one canonical .rilhm fragment without installing managed handlers.
        public static byte[] Build(RilsHostModuleDescriptor module)
        {
            if (module == null) throw new ArgumentNullException(nameof(module));
            if (module.Version != 1)
            {
                throw new NotSupportedException("The current native host registration API only supports module version 1.");
            }
            using (var runtime = new RilsRuntime())
            {
                for (int index = 0; index < module.Types.Count; index++)
                {
                    RilsHostDeclarationInterop.Register(runtime, module.Types[index]);
                }
                for (int index = 0; index < module.Functions.Count; index++)
                {
                    RilsHostDeclarationInterop.Register(runtime, module.Functions[index]);
                }
                return runtime.GetHostManifest();
            }
        }
    }
}
