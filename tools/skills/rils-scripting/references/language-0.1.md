# Rils 0.1 language baseline

Use this reference when writing or reviewing `.rils` files. Project-local language documentation takes precedence if the host uses a newer Rils release.

## Core forms

```rust
struct Counter { value: i32 }

impl Counter {
    fn increment(&mut self) {
        self.value = self.value + 1;
    }
}

fn calculate(input: i32) -> Result<i32, string> {
    if input < 0 {
        Err("input must be non-negative")
    } else {
        Ok(input * 2)
    }
}

fn run(counter: &Counter) -> Result<i32, string> {
    let output = calculate(counter.value)?;
    Ok(output)
}

let mut counter = Counter { value: 0 };
counter.increment();
let output = unwrap(run(&counter));
```

Blocks, `if`, `match`, and `loop` are expressions. A trailing semicolon changes an expression result to `()`.

Primitive and common types:

- `()`, `bool`, `char`, `string`
- signed integers `i8`, `i16`, `i32`, `i64`, `i128`, `isize`
- unsigned integers `u8`, `u16`, `u32`, `u64`, `u128`, `usize`
- floating-point types `f32`, `f64`
- `Option<T>` with `Some(value)` and `None`
- `Result<T, E>` with `Ok(value)` and `Err(error)`
- `(A, B)`, `[T; N]`, and `Vec<T>`
- `fn(A, B) -> R` and compatibility type `function`
- named `struct` and `enum` types

The former `int` and `float` names are removed rather than aliases. An unconstrained integer literal defaults to
`i32` and an unconstrained floating literal defaults to `f64`; suffixes and surrounding usage may select another
concrete type. Index and collection-length contexts infer `usize`, so `let index = 1; values[index]` does not need
an explicit annotation. `Vec::len()` returns `usize`.

There is no `nil`. Annotate an otherwise unconstrained `None`, such as `let item: Option<i32> = None;`.

## Ownership

`()`、`bool`, every concrete integer/float type, `char`, and composite values containing only Copy data are Copy.
Strings, collections, closures, and composites containing non-Copy data move by default.

```rust
let text = "hello";
let moved = text;
// text is no longer usable

let source = "world";
let copied = clone(&source);
```

`clone` receives a reference. Passing or returning a non-Copy value and assigning it to another binding also move it.

## References

`&T` is a local shared reference and `&mut T` is a local writable reference. Multiple writable references to the same place are permitted.

```rust
fn add(value: &mut i32, amount: i32) {
    *value = *value + amount;
}

let mut total = 0;
{
    let first = &mut total;
    let second = &mut total;
    add(first, 20);
    add(second, 22);
}
```

References cannot be global, returned, captured, used as block or match results, or stored in tuples, arrays, `Vec`, `Option`, `Result`, structs, or enums. An active reference prevents moving or replacing its owner.

## Control flow and collections

Use `while`, `loop`, or `for`. `break value` gives a loop a result and `continue` starts the next iteration. `start..end` is an integer half-open range.

Owned iteration consumes the collection:

```rust
let values = Vec::from([20, 22]);
let mut total = 0;
for value in values {
    total = total + value;
}
```

`Vec<T>` provides `new`, `from`, `len`, `push`, and `pop`; `len` returns `usize` and `pop` returns `Option<T>`.
Array and Vec indexes use `usize`. Borrowed container iterators are not part of the 0.1 baseline.

## Data and matching

Structs use named fields and enums support unit, tuple, and record variants:

```rust
enum Message {
    Quit,
    Move(i32, i32),
    Write { text: string },
}

fn describe(message: Message) -> string {
    match message {
        Message::Quit => "quit",
        Message::Move(x, y) => if x == y { "diagonal" } else { "move" },
        Message::Write { text } => text,
    }
}
```

Known `bool`, Option, Result, and enum matches are checked for exhaustiveness. Record patterns must list every field; `..`, guards, or-patterns, and `@` bindings are unavailable.

## Methods, traits, and generics

Receivers are `self`, `mut self`, `&self`, or `&mut self`. Inherent methods take priority. Use UFCS when multiple traits provide the same method:

```rust
<Number as Describe>::describe(&number)
```

UFCS does not auto-borrow. Generic parameters support `T: Trait + OtherTrait`. `Self` identifies the concrete impl type. Associated types are supported, including qualified projection such as `<Both as Left>::Item`.

Unavailable in 0.1: trait objects, default trait method bodies, generic traits, `where`, conditional impls, const generics, explicit turbofish, and lifetime parameters.

## Functions and closures

Nested `fn` declarations form lexical closures and may capture owned bindings. References cannot be captured.

```rust
fn make_counter() -> fn() -> i32 {
    let mut count = 0;
    fn next() -> i32 {
        count = count + 1;
        count
    }
    next
}
```

## Modules and capabilities

Use `mod name { ... }` for inline modules and `mod name;` for `name.rils` or `name/mod.rils`. Export with `pub`; import with `use path::item` or `use path::item as alias`. Glob/group imports and `crate`, `self`, or `super` paths are unavailable.

The standard hierarchy is `core`, `std`, and `prelude`. Host/platform functions belong under explicit modules. IO and filesystem failures return `Result`; access may be denied by the host capability policy.

Built-in macros are `print!`, `println!`, and `assert!`. User token-tree macros support `expr`, `lit`, and `ident` captures, simple repetition, and a maximum expansion depth of 64; they are currently non-hygienic.
