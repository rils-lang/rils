using Rils.CSharp;

static void Equal<T>(T expected, T actual, string label)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"{label}: expected {expected}, found {actual}");
    }
}

Equal(3U, RilsRuntime.NativeAbiVersion, "ABI version");

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
        """,
        "managed-smoke.rils");
    using RilsInstance instance = module.CreateInstance();

    Equal(42, instance.Call("add", 20, 22).AsI32(), "i32 call");

    var signed128 = new RilsInt128(0x0123456789ABCDEF, -123456789);
    Equal(signed128, instance.Call("echo_i128", RilsValue.From(signed128)).AsI128(), "i128 call");

    var scalar = new RilsChar(0x1F642);
    Equal(scalar, instance.Call("echo_char", RilsValue.From(scalar)).AsChar(), "char call");
    Equal(1.25F, instance.Call("echo_f32", 1.25F).AsF32(), "f32 call");

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
