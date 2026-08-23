#nullable enable
using System;
using System.Collections.Generic;
using System.Text;

namespace Rils.CSharp
{
    public enum RilsHostValueFieldType
    {
        Bool,
        I8,
        I16,
        I32,
        I64,
        I128,
        U8,
        U16,
        U32,
        U64,
        U128,
        F32,
        F64,
    }

    public sealed class RilsHostValueLayout : IEquatable<RilsHostValueLayout>
    {
        private const int MaxPayloadBytes = 16;
        private const int MaxFields = 16;
        private readonly RilsHostValueFieldType[] _fields;

        private RilsHostValueLayout(params RilsHostValueFieldType[] fields)
        {
            if (fields == null) throw new ArgumentNullException(nameof(fields));
            if (fields.Length == 0)
                throw new ArgumentException("An inline host value layout must declare at least one field.", nameof(fields));
            if (fields.Length > MaxFields)
                throw new ArgumentException($"An inline host value layout cannot exceed {MaxFields} fields.", nameof(fields));
            _fields = (RilsHostValueFieldType[])fields.Clone();
            int byteLength = 0;
            for (int index = 0; index < _fields.Length; index++)
            {
                byteLength = checked(byteLength + FieldByteLength(_fields[index]));
            }
            if (byteLength > MaxPayloadBytes)
                throw new ArgumentException(
                    $"The inline host value layout requires {byteLength} bytes, exceeding the {MaxPayloadBytes}-byte ABI payload.",
                    nameof(fields));
            ByteLength = byteLength;
            var names = new string[_fields.Length];
            for (int index = 0; index < _fields.Length; index++) names[index] = FieldName(_fields[index]);
            CanonicalName = $"fields({string.Join(",", names)})";
        }

        public static RilsHostValueLayout F32x2 { get; } =
            FromFields(RilsHostValueFieldType.F32, RilsHostValueFieldType.F32);
        public static RilsHostValueLayout F32x3 { get; } =
            FromFields(RilsHostValueFieldType.F32, RilsHostValueFieldType.F32, RilsHostValueFieldType.F32);
        public static RilsHostValueLayout F32x4 { get; } =
            FromFields(RilsHostValueFieldType.F32, RilsHostValueFieldType.F32,
                RilsHostValueFieldType.F32, RilsHostValueFieldType.F32);

        public static RilsHostValueLayout FromFields(params RilsHostValueFieldType[] fields) =>
            new RilsHostValueLayout(fields);

        public IReadOnlyList<RilsHostValueFieldType> Fields => Array.AsReadOnly(_fields);
        public int ByteLength { get; }
        public string CanonicalName { get; }

        public bool Equals(RilsHostValueLayout? other)
        {
            if (ReferenceEquals(this, other)) return true;
            if (other == null || _fields.Length != other._fields.Length) return false;
            for (int index = 0; index < _fields.Length; index++)
            {
                if (_fields[index] != other._fields[index]) return false;
            }
            return true;
        }

        public override bool Equals(object? obj) => Equals(obj as RilsHostValueLayout);
        public override int GetHashCode()
        {
            unchecked
            {
                int hash = 17;
                for (int index = 0; index < _fields.Length; index++) hash = hash * 31 + (int)_fields[index];
                return hash;
            }
        }

        private static string FieldName(RilsHostValueFieldType field) => field switch
        {
            RilsHostValueFieldType.Bool => "bool",
            RilsHostValueFieldType.I8 => "i8",
            RilsHostValueFieldType.I16 => "i16",
            RilsHostValueFieldType.I32 => "i32",
            RilsHostValueFieldType.I64 => "i64",
            RilsHostValueFieldType.I128 => "i128",
            RilsHostValueFieldType.U8 => "u8",
            RilsHostValueFieldType.U16 => "u16",
            RilsHostValueFieldType.U32 => "u32",
            RilsHostValueFieldType.U64 => "u64",
            RilsHostValueFieldType.U128 => "u128",
            RilsHostValueFieldType.F32 => "f32",
            RilsHostValueFieldType.F64 => "f64",
            _ => throw new ArgumentOutOfRangeException(nameof(field)),
        };

        private static int FieldByteLength(RilsHostValueFieldType field) => field switch
        {
            RilsHostValueFieldType.Bool or RilsHostValueFieldType.I8 or RilsHostValueFieldType.U8 => 1,
            RilsHostValueFieldType.I16 or RilsHostValueFieldType.U16 => 2,
            RilsHostValueFieldType.I32 or RilsHostValueFieldType.U32 or RilsHostValueFieldType.F32 => 4,
            RilsHostValueFieldType.I64 or RilsHostValueFieldType.U64 or RilsHostValueFieldType.F64 => 8,
            RilsHostValueFieldType.I128 or RilsHostValueFieldType.U128 => 16,
            _ => throw new ArgumentOutOfRangeException(nameof(field)),
        };
    }

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
                throw new NotSupportedException("Opaque host types must use HostHandle transport.");
            Name = name;
            BaseTypeName = baseTypeName;
            TransportTag = transportTag;
            Kind = RilsHostTypeKind.Opaque;
            ValueLayout = null;
        }

        private RilsHostTypeDescriptor(string name, RilsHostValueLayout valueLayout)
        {
            if (string.IsNullOrWhiteSpace(name))
                throw new ArgumentException("Host type name cannot be empty.", nameof(name));
            Name = name;
            BaseTypeName = null;
            TransportTag = RilsValueTag.InlineValue;
            Kind = RilsHostTypeKind.Value;
            ValueLayout = valueLayout;
        }

        public static RilsHostTypeDescriptor InlineValue(
            string name,
            RilsHostValueLayout valueLayout) => new RilsHostTypeDescriptor(name, valueLayout);

        public string Name { get; }
        public string? BaseTypeName { get; }
        public RilsValueTag TransportTag { get; }
        public RilsHostTypeKind Kind { get; }
        public RilsHostValueLayout? ValueLayout { get; }

        internal string? ValueLayoutName => ValueLayout?.CanonicalName;
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
            var overloads = new HashSet<string>(StringComparer.Ordinal);
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
                if (!overloads.Add(FunctionOverloadKey(function)))
                {
                    throw new ArgumentException(
                        $"Host function '{function.Name}' has a duplicated mapped parameter signature in module '{name}'.",
                        nameof(functions));
                }
                ValidateLogicalType(function.ReturnParameter, typeNames, function.Name);
                for (int parameterIndex = 0; parameterIndex < function.Parameters.Count; parameterIndex++)
                {
                    ValidateLogicalType(function.Parameters[parameterIndex], typeNames, function.Name);
                }
            }
            Functions = Array.AsReadOnly(functionSnapshot);
        }

        private static string FunctionOverloadKey(RilsHostFunctionDescriptor function)
        {
            var key = new System.Text.StringBuilder(function.Name);
            key.Append('\0');
            for (int index = 0; index < function.Parameters.Count; index++)
            {
                RilsHostParameter parameter = function.Parameters[index];
                key.Append((int)parameter.Tag).Append(':')
                    .Append((int)parameter.TransferMode).Append(':')
                    .Append(parameter.LogicalTypeName).Append(';');
            }
            return key.ToString();
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
            return Build(module.Types, module.Functions);
        }

        /// Builds one canonical .rilhm fragment from declarations that may span modules.
        public static byte[] Build(
            IReadOnlyList<RilsHostTypeDescriptor> types,
            IReadOnlyList<RilsHostFunctionDescriptor> functions)
        {
            if (types == null) throw new ArgumentNullException(nameof(types));
            if (functions == null) throw new ArgumentNullException(nameof(functions));
            using (var runtime = new RilsRuntime())
            {
                for (int index = 0; index < types.Count; index++)
                {
                    RilsHostDeclarationInterop.Register(runtime, types[index]);
                }
                for (int index = 0; index < functions.Count; index++)
                {
                    RilsHostDeclarationInterop.Register(runtime, functions[index]);
                }
                return runtime.GetHostManifest();
            }
        }
    }
}
