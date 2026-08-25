using Rils.CSharp;

static void Equal<T>(T expected, T actual, string label)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"{label}: expected {expected}, found {actual}");
    }
}

Equal(7U, RilsRuntime.NativeAbiVersion, "ABI version");

using (var runtime = new RilsRuntime())
{
    var output = new List<(string Text, bool Newline)>();
    runtime.SetOutputHandler((text, newline) => output.Add((text, newline)));
    runtime.AllowStandardLibrary();
    using RilsModule module = runtime.Compile(
        "print!(\"value={}\", 7); println!(\" done\");",
        "managed-output-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    instance.Execute();
    Equal(2, output.Count, "managed output event count");
    Equal(("value=7", false), output[0], "managed print callback");
    Equal((" done", true), output[1], "managed println callback");
}

ulong stableHostId = RilsHostStableId.FromCanonicalName(
    "UnityEngine.CoreModule:UnityEngine.Time.get_deltaTime():System.Single");
Equal(
    stableHostId,
    RilsHostStableId.FromCanonicalName(
        "UnityEngine.CoreModule:UnityEngine.Time.get_deltaTime():System.Single"),
    "stable host ID");
var timeDescriptor = new RilsHostFunctionDescriptor(
    stableHostId,
    "unity_engine::time::delta_time",
    "unity_engine.time",
    new RilsHostParameter(RilsValueTag.F32),
    Array.Empty<RilsHostParameter>(),
    managedMemberName: "UnityEngine.Time.get_deltaTime");
var timeModule = new RilsHostModuleDescriptor(
    "unity_engine::time",
    1,
    new[] { timeDescriptor });
var mutableParameters = new[] { new RilsHostParameter(RilsValueTag.I32) };
var immutableFunction = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.Immutable(System.Int32):System.Void"),
    "smoke::immutable",
    "smoke.immutable",
    new RilsHostParameter(RilsValueTag.Unit),
    mutableParameters);
mutableParameters[0] = new RilsHostParameter(RilsValueTag.Bool);
Equal(RilsValueTag.I32, immutableFunction.Parameters[0].Tag, "function parameter snapshot");
var mutableFunctions = new[] { immutableFunction };
var immutableModule = new RilsHostModuleDescriptor("smoke", 1, mutableFunctions);
mutableFunctions[0] = timeDescriptor;
Equal("smoke::immutable", immutableModule.Functions[0].Name, "module function snapshot");
var integerOverload = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.Pick(System.Int32):System.Int32"),
    "smoke::pick",
    "smoke.overload",
    new RilsHostParameter(RilsValueTag.I32),
    new[] { new RilsHostParameter(RilsValueTag.I32) });
var floatOverload = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.Pick(System.Single):System.Single"),
    "smoke::pick",
    "smoke.overload",
    new RilsHostParameter(RilsValueTag.F32),
    new[] { new RilsHostParameter(RilsValueTag.F32) });
var overloadModule = new RilsHostModuleDescriptor(
    "smoke",
    1,
    new[] { integerOverload, floatOverload });
Equal(2, overloadModule.Functions.Count, "host overload descriptors");
try
{
    _ = new RilsHostModuleDescriptor(
        "smoke",
        1,
        new[]
        {
            integerOverload,
            new RilsHostFunctionDescriptor(
                RilsHostStableId.FromCanonicalName("Smoke.PickDuplicate(System.Int32):System.String"),
                "smoke::pick",
                "smoke.overload",
                new RilsHostParameter(RilsValueTag.Bool),
                new[] { new RilsHostParameter(RilsValueTag.I32) }),
        });
    throw new InvalidOperationException("mapped overload collision was accepted");
}
catch (ArgumentException error) when (error.Message.Contains("mapped parameter signature"))
{
}
byte[] timeManifest = RilsHostManifestBuilder.Build(timeModule);
using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(timeManifest);
    using var hosts = new RilsHostRegistry(runtime);
    hosts.Register(new RilsHostFunction(timeDescriptor, _ => RilsValue.From(0.25F)));
    hosts.AllowCapability("unity_engine.time");
    hosts.Freeze();
    using RilsModule module = runtime.Compile(
        "unity_engine::time::delta_time()",
        "managed-host-module-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    Equal(0.25F, instance.Execute().AsF32(), "descriptor-backed host dispatch");
    runtime.Dispose();
}

using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(RilsHostManifestBuilder.Build(overloadModule));
    using var hosts = new RilsHostRegistry(runtime);
    hosts.Register(new RilsHostFunction(
        integerOverload,
        arguments => RilsValue.From(arguments[0].AsI32() + 1)));
    hosts.Register(new RilsHostFunction(
        floatOverload,
        arguments => RilsValue.From(arguments[0].AsF32() + 0.5F)));
    hosts.AllowCapability("smoke.overload");
    hosts.Freeze();
    using RilsModule module = runtime.Compile(
        "let selected: i32 = smoke::pick(20i32); " +
        "let ignored: f32 = smoke::pick(2.0f32); selected",
        "managed-host-overload-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    Equal(21, instance.Execute().AsI32(), "descriptor-backed host overload dispatch");
    runtime.Dispose();
}

var cameraType = RilsHostTypeDescriptor.Enum(
    "unity_engine::CameraType",
    RilsValueTag.I32,
    flags: false,
    new[]
    {
        new RilsHostEnumVariantDescriptor("Game", 1),
        new RilsHostEnumVariantDescriptor("SceneView", 2),
    });
RilsHostParameter cameraParameter = RilsHostParameter.NamedEnum(
    "unity_engine::CameraType",
    RilsValueTag.I32);
var cameraEchoDescriptor = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.CameraEcho(UnityEngine.CameraType):UnityEngine.CameraType"),
    "unity_engine::camera::echo",
    "unity.camera",
    cameraParameter,
    new[] { cameraParameter });
var cameraModule = new RilsHostModuleDescriptor(
    "unity_engine::camera",
    1,
    new[] { cameraType },
    new[] { cameraEchoDescriptor });
using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(RilsHostManifestBuilder.Build(cameraModule));
    using var hosts = new RilsHostRegistry(runtime);
    hosts.Register(new RilsHostFunction(cameraEchoDescriptor, arguments => arguments[0]));
    hosts.AllowCapability("unity.camera");
    hosts.Freeze();
    using RilsModule module = runtime.Compile(
        "match unity_engine::camera::echo(unity_engine::CameraType::Game) { " +
        "unity_engine::CameraType::Game => true, _ => false }",
        "managed-host-enum-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    Equal(true, instance.Execute().AsBool(), "real host enum dispatch");
    runtime.Dispose();
}

RilsHostParameter gameObjectType = RilsHostParameter.NamedHandle("unity_engine::GameObject");
Equal(RilsValueTag.HostHandle, gameObjectType.Tag, "named handle transport");
Equal("unity_engine::GameObject", gameObjectType.LogicalTypeName, "named handle logical type");

var objectType = new RilsHostTypeDescriptor("unity_engine::Object");
var derivedGameObjectType = new RilsHostTypeDescriptor(
    "unity_engine::GameObject",
    "unity_engine::Object");
var getObjectDescriptor = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.GetGameObject():UnityEngine.GameObject"),
    "unity_engine::object::get",
    "unity_engine.object",
    RilsHostParameter.NamedHandle("unity_engine::GameObject"),
    Array.Empty<RilsHostParameter>());
var instanceIdDescriptor = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("UnityEngine.Object.GetInstanceID():System.Int32"),
    "unity_engine::object::instance_id",
    "unity_engine.object",
    new RilsHostParameter(RilsValueTag.I64),
    new[] { RilsHostParameter.NamedHandle("unity_engine::Object") },
    receiver: RilsHostReceiver.RefSelf);
var objectModule = new RilsHostModuleDescriptor(
    "unity_engine::object",
    1,
    new[] { objectType, derivedGameObjectType },
    new[] { getObjectDescriptor, instanceIdDescriptor });
byte[] objectManifest = RilsHostManifestBuilder.Build(objectModule);
Equal(5U, BitConverter.ToUInt32(objectManifest, 8), "host manifest format version");
using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(objectManifest);
    using var hosts = new RilsHostRegistry(runtime);
    hosts.Register(new RilsHostFunction(
        getObjectDescriptor,
        _ => RilsValue.From(new RilsObjectHandle(1, 77, 3, 9))));
    hosts.Register(new RilsHostFunction(
        instanceIdDescriptor,
        arguments => RilsValue.From(arguments[0].AsHostHandle(1).ObjectId)));
    hosts.AllowCapability("unity_engine.object");
    hosts.Freeze();
    using RilsModule module = runtime.Compile(
        "unity_engine::object::get().instance_id()",
        "managed-named-host-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    Equal(77L, instance.Execute().AsI64(), "inherited named host method dispatch");
    using RilsModule behaviourModule = runtime.Compile(
        """
        trait Behaviour: Default {
            fn tick(&mut self, host: unity_engine::GameObject) -> i64;
        }
        #[derive(Default)]
        struct State;
        impl Behaviour for State {
            fn tick(&mut self, host: unity_engine::GameObject) -> i64 { 42i64 }
        }
        """,
        "managed-named-trait-argument-smoke.rils");
    using RilsInstance behaviourInstance = behaviourModule.CreateInstance();
    using RilsScriptValue state = behaviourInstance.CreateDefaultValue("State");
    Equal(
        42L,
        state.CallTraitTyped(
            "Behaviour",
            "tick",
            RilsHostArgument.NamedHandle(new RilsObjectHandle(1, 77, 3, 9), "unity_engine::GameObject"))
            .AsI64(),
        "named host trait argument");
    runtime.Dispose();
}

var vectorType = RilsHostTypeDescriptor.InlineValue(
    "unity_engine::Vector3",
    RilsHostValueLayout.F32x3);
var mixedLayout = RilsHostValueLayout.FromFields(
    RilsHostValueFieldType.U32,
    RilsHostValueFieldType.U64,
    RilsHostValueFieldType.U32);
Equal("fields(u32,u64,u32)", mixedLayout.CanonicalName, "general inline layout name");
Equal(16, mixedLayout.ByteLength, "general inline layout size");
var mixedWriter = new RilsInlineValueWriter();
mixedWriter.WriteU32(0x11223344U);
mixedWriter.WriteU64(0x5566778899AABBCCUL);
mixedWriter.WriteU32(0xDDEEFF00U);
var mixedReader = new RilsInlineValueReader(mixedWriter.Build());
Equal(0x11223344U, mixedReader.ReadU32(), "mixed inline first field");
Equal(0x5566778899AABBCCUL, mixedReader.ReadU64(), "mixed inline crossing field");
Equal(0xDDEEFF00U, mixedReader.ReadU32(), "mixed inline last field");
RilsHostParameter vectorParameter = RilsHostParameter.NamedValue("unity_engine::Vector3");
var vectorNewDescriptor = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.Vector3.New(System.Single,System.Single,System.Single)"),
    "unity_engine::vector3::new",
    "unity.math",
    vectorParameter,
    new[]
    {
        new RilsHostParameter(RilsValueTag.F32),
        new RilsHostParameter(RilsValueTag.F32),
        new RilsHostParameter(RilsValueTag.F32),
    });
var vectorSumDescriptor = new RilsHostFunctionDescriptor(
    RilsHostStableId.FromCanonicalName("Smoke.Vector3.ComponentSum(UnityEngine.Vector3)"),
    "unity_engine::vector3::component_sum",
    "unity.math",
    new RilsHostParameter(RilsValueTag.F32),
    new[] { vectorParameter });
var vectorModule = new RilsHostModuleDescriptor(
    "unity_engine::vector3",
    1,
    new[] { vectorType },
    new[] { vectorNewDescriptor, vectorSumDescriptor });
byte[] vectorManifest = RilsHostManifestBuilder.Build(vectorModule);
Equal(5U, BitConverter.ToUInt32(vectorManifest, 8), "inline value manifest format version");
using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(vectorManifest);
    using var hosts = new RilsHostRegistry(runtime);
    hosts.Register(new RilsHostFunction(
        vectorNewDescriptor,
        arguments => RilsValue.From(RilsInlineValue.FromF32(
            arguments[0].AsF32(),
            arguments[1].AsF32(),
            arguments[2].AsF32()))));
    hosts.Register(new RilsHostFunction(
        vectorSumDescriptor,
        arguments =>
        {
            RilsInlineValue vector = arguments[0].AsInlineValue();
            return RilsValue.From(vector.GetF32(0) + vector.GetF32(1) + vector.GetF32(2));
        }));
    hosts.AllowCapability("unity.math");
    hosts.Freeze();
    using RilsModule module = runtime.Compile(
        "unity_engine::vector3::component_sum(unity_engine::vector3::new(1.25f32, 2.5f32, 3.75f32))",
        "managed-inline-value-smoke.rils");
    using RilsInstance instance = module.CreateInstance();
    Equal(7.5F, instance.Execute().AsF32(), "inline value host dispatch");
    runtime.Dispose();
}

byte[] emptyHostManifest;
using (var runtime = new RilsRuntime())
{
    emptyHostManifest = runtime.GetHostManifest();
    Equal(true, emptyHostManifest.Length >= 64, "binary host manifest header size");
    Equal((byte)'R', emptyHostManifest[0], "binary host manifest magic");
}

using (var runtime = new RilsRuntime())
{
    using RilsModule module = runtime.Compile(
        """
        trait Behaviour: Default { fn tick(&mut self, amount: i32) -> i32; }
        #[derive(Default)]
        struct State { value: i32 }
        impl Behaviour for State {
            fn tick(&mut self, amount: i32) -> i32 {
                self.value = self.value + amount;
                self.value
            }
        }
        """,
        "managed-trait-smoke.rils");
    Equal("State", module.GetTraitImplementations("Behaviour").Single(), "trait entry");
    using RilsInstance instance = module.CreateInstance();
    using RilsScriptValue state = instance.CreateDefaultValue("State");
    Equal(2, state.CallTrait("Behaviour", "tick", 2).AsI32(), "first trait call");
    Equal(5, state.CallTrait("Behaviour", "tick", 3).AsI32(), "persistent trait call");
}
using (var runtime = new RilsRuntime())
{
    runtime.RegisterHostManifest(emptyHostManifest);
    Equal(
        true,
        emptyHostManifest.SequenceEqual(runtime.GetHostManifest()),
        "binary host manifest round trip");
}

using (var runtime = new RilsRuntime())
{
    runtime.SetMaxSteps(100_000);
    using RilsModule module = runtime.Compile(
        """
        pub fn add(left: i32, right: i32) -> i32 { left + right }
        pub fn echo_i128(value: i128) -> i128 { value }
        pub fn echo_char(value: char) -> char { value }
        pub fn echo_f32(value: f32) -> f32 { value }
        pub fn echo_string(value: string) -> string { value }
        """,
        "managed-smoke.rils");
    using RilsInstance instance = module.CreateInstance();

    Equal(42, instance.Call("add", 20, 22).AsI32(), "i32 call");

    var signed128 = new RilsInt128(0x0123456789ABCDEF, -123456789);
    Equal(signed128, instance.Call("echo_i128", RilsValue.From(signed128)).AsI128(), "i128 call");

    var scalar = new RilsChar(0x1F642);
    Equal(scalar, instance.Call("echo_char", RilsValue.From(scalar)).AsChar(), "char call");
    Equal(1.25F, instance.Call("echo_f32", 1.25F).AsF32(), "f32 call");
    Equal("你好，Rils 👋", instance.Call("echo_string", "你好，Rils 👋").AsString(), "string call");

    byte[] image = module.GetBytecode();
    using RilsModule loadedModule = runtime.LoadBytecode(image);
    using RilsInstance loadedInstance = loadedModule.CreateInstance();
    Equal(42, loadedInstance.Call("add", 20, 22).AsI32(), "exported bytecode call");
}

using (var runtime = new RilsRuntime())
{
    try
    {
        runtime.LoadBytecode(new byte[] { 1, 2, 3, 4 });
        throw new InvalidOperationException("invalid bytecode did not throw RilsException");
    }
    catch (RilsException error)
    {
        Equal(RilsStatus.BytecodeError, error.Status, "bytecode status");
        Equal("<bytecode>", error.SourceName, "bytecode source name");
    }
}

string moduleDirectory = Path.Combine(Path.GetTempPath(), $"rils-csharp-modules-{Environment.ProcessId}");
Directory.CreateDirectory(moduleDirectory);
string entryPath = Path.Combine(moduleDirectory, "main.rils");
string dependencyPath = Path.Combine(moduleDirectory, "math.rils");
string behaviourPath = Path.Combine(moduleDirectory, "behaviour.rils");
string bytecodePath = Path.Combine(moduleDirectory, "main.rilbc");
try
{
    File.WriteAllText(entryPath, "mod math; use math::answer; answer()");
    File.WriteAllText(dependencyPath, "pub fn answer() -> i32 { 42 }");
    File.WriteAllText(
        behaviourPath,
        "trait Behaviour: Default { fn tick(&mut self); } " +
        "#[derive(Default)] struct State; " +
        "impl Behaviour for State { fn tick(&mut self) { } }");
    using var runtime = new RilsRuntime();
    using RilsModule module = runtime.CompileFile(entryPath);
    using RilsInstance instance = module.CreateInstance();
    Equal(42, instance.Execute().AsI32(), "file module execution");
    module.WriteBytecodeFile(bytecodePath);
    using RilsModule loadedModule = runtime.LoadBytecodeFile(bytecodePath);
    using RilsInstance loadedInstance = loadedModule.CreateInstance();
    Equal(42, loadedInstance.Execute().AsI32(), "exported file execution");
    using RilsModule behaviourModule = runtime.CompileFile(behaviourPath);
    Equal(
        "State",
        behaviourModule.GetTraitImplementations("Behaviour", behaviourPath).Single(),
        "source-filtered trait entry");
    Equal(
        0,
        behaviourModule.GetTraitImplementations("Behaviour", dependencyPath).Count,
        "unrelated source trait entries");
}
finally
{
    File.Delete(bytecodePath);
    File.Delete(entryPath);
    File.Delete(dependencyPath);
    File.Delete(behaviourPath);
    Directory.Delete(moduleDirectory);
}

using (var runtime = new RilsRuntime())
{
    try
    {
        runtime.Compile("let = 1;", "broken.rils");
        throw new InvalidOperationException("compile failure did not throw RilsException");
    }
    catch (RilsException error)
    {
        Equal(RilsStatus.CompileError, error.Status, "compile status");
        Equal("broken.rils", error.SourceName, "compile source name");
    }
}

var owner = new RilsRuntime();
RilsModule childModule = owner.Compile("pub fn answer() -> i32 { 42 }", "lifecycle.rils");
RilsInstance childInstance = childModule.CreateInstance();
owner.Dispose();
Equal(true, childModule.IsDisposed, "module cascade disposal");
Equal(true, childInstance.IsDisposed, "instance cascade disposal");

Console.WriteLine("Rils.CSharp managed smoke tests passed.");
