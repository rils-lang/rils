# rils_stdlib_macros

Procedural macros for Rust-backed Rils standard-library declarations. This crate owns `decl_rils`, declaration metadata, source, layout, native binding generation, and type-pattern parsing. Legacy built-in ID and catalog generation remain in `rils_builtins_macros`.

## Implementation-only sequence storage

An exported collection declares its language type parameters followed by one implementation-only Rust storage parameter:

```rust,ignore
#[rils_struct]
pub struct Vec<
    T,
    #[rils_storage(sequence, rils_stdlib::stdlib::collections::vector::Vec)]
    S = std::vec::Vec<T>,
>(S, std::marker::PhantomData<T>);
```

The marker explicitly selects the sequence layout and supplies the public Rust receiver path for generated calls. The backend needs a Rust default; it is omitted from generated Rils source, catalog type parameters, and trait declaration matching. It must not appear in generated Rils signatures or public fields. Rust-only constructors and storage bounds remain ordinary Rust code.

Declarations with multiple language type parameters specify their stored entry type explicitly:

```rust,ignore
#[rils_struct]
pub struct Map<
    K,
    V,
    #[rils_storage(sequence((K, V)), crate::Map)]
    S = std::collections::HashMap<K, V>,
>(S, std::marker::PhantomData<(K, V)>);
```

The explicit element may be a language type parameter or a tuple of those parameters, including nested or reordered tuples. Every language parameter must occur exactly once; omitted and repeated parameters are rejected. Source, metadata, associated type projection, and generated layouts preserve every language parameter and omit only the trailing backend. Tuple fields retain numeric names (`0`, `1`, ...), so `(K, V)` has the same entry layout as the existing native maps. Omitting the element expression is supported only with one language type parameter. The generated `EntrySchema` describes these parameter positions for argument conversion, output layouts, and borrowed iterator projections. Parameter identity is retained even when different parameters instantiate to the same concrete type.

The receiver provides `with_storage(S) -> Self`. Generated calls specialize owned language parameters to `DynamicValue`, borrowed query parameters to a query adapter, and storage to validated `SharedAccess` or `MutableAccess` views. Query and mutation bodies remain generic over their backend. `SequenceRead` supplies length metadata, including indexed reads of fixed arrays; calls between query methods execute normally. No method body is copied into the execution crate, and no method-name registration is needed.

The generic call adapter handles shared or mutable receivers, `usize` arguments, owned or borrowed language parameters, and owned or borrowed `Self` inputs. At most one shared `Self` argument is supported, and it requires a shared receiver. Results may be `()`, `bool`, `usize`, a language parameter, `Option<parameter>`, an owning iterator of a parameter, or `Self`. The native specialization is checked against the required Rust output representation; `#[rils_return(...)]` preserves the language signature when Rust returns an associated backend output. Input mappings use `#[rils_param(...)]` as described below. Backend types cannot leak into public fields or exported language signatures.

`SequenceTruncate` supplies suffix removal. Its `TruncateOutput` is `()` for Rust Vec and `Result<(), String>` for Rils mutable views. Standard-library `clear` calls `truncate(0)` using the same method bodies on both backends. Borrow checks and destruction remain in the storage implementation; the execution bridge requires a mutable reference and rejects fixed arrays.

`SequenceTransfer<T>` covers push, insert, pop, remove, and swap-remove. Rust Vec keeps its existing return types; Rils storage uses `DynamicValue` transfers and fallible results. Signatures with owned element inputs require an owned argument frame. Arguments are decoded before obtaining the mutable view, in declaration order; conversion or mutation failures release consumed inputs normally. The execution adapter retains the receiver, converts elements, and wraps output values, including the concrete element layout of an empty Option. It does not choose collection operations by name.

Borrowed language-parameter arguments retain their original query value and declaration context. `SequenceSearch<T>` uses `ElementQuery` for Vec equality and preserves short-circuiting and comparison errors. Map/Set use `KeyQuery` with a `KeyPolicy` selected by their standard-library method bodies. Hash keys require nominal `Eq + Hash` recursively; named enum payload records share their enum declaration. Ordered keys retain the native registry's leaf ordering restrictions. Execution supplies generic trait metadata and cloning through `NativeServices`; it does not choose a collection policy or copy query values.

Owned `Self` inputs also use the generated mutable-call path. The call frame retains each owned container and lends its mutable view only during the method call; aliases of the receiver, borrowed inputs, and incompatible layouts are rejected before mutation. `SequenceExtend` moves elements between storage backends, so `Vec::extend(&mut self, other: Self)` executes its declared body without a method-name branch, an argument-type override, or a temporary element array. Source and destination borrow checks precede element transfer.

Shared zero-argument methods declaring an iterator of references execute their bodies through `SequenceIter`. Its lifetime-indexed output preserves Rust borrows. The language item shape may contain nested tuples of shared references, such as `(&K, &V)`; projections combine the declared type-parameter identity with `EntrySchema`, rather than guessing fields from concrete types. The execution backend retains the lexical source, codec, and structural-mutation lease. These methods use borrowed dispatch. Direct Rust dispatch should use `requires_owned_native_call(symbol)` to select the appropriate call entry point.

Storage-backed exported trait impls can include the hidden backend parameter. Bounds directly on that parameter remain Rust-only; bounds on language parameters remain part of the declaration. An associated type can specify its language representation at its definition:

```rust,ignore
#[rils_impl]
impl<T, S: SequenceIntoIter> IntoIterator for Vec<T, S> {
    #[rils_associated(T)]
    type Item = S::Item;
    #[rils_associated(Iterator<T>)]
    type IntoIter = S::IntoIter;
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}
```

The source and metadata use the associated language type, while Rust retains `S::IntoIter`. Backend types cannot leak through trait requirements, associated language types, or method signatures. Duplicate overrides and overrides outside exported storage-backed trait impls are rejected; trait method return overrides remain forbidden.

Owning zero-argument `IntoIterator` methods execute the original body using `SequenceIntoIter`. The native call prepares an `OwnedElements` cursor after ownership and borrow checks; its Rust item is `Result<DynamicValue, String>`, preserving lazy failures at the execution boundary without changing the declared language item type. The body must return `OwnedElements`; the adapter checks its entry layout and retains its codec and declared iterator type. Arrays, Vec, VecDeque, BinaryHeap, HashMap, BTreeMap, HashSet, and BTreeSet share this path. The cursor is never converted to an opaque generator, so remaining-element traversal, lexical references, and destruction stay intact. These protocols do not change the physical Rils storage layout.

`VecDeque<T, S>` reuses the sequence backend marker. `SequenceDeque<T>` supplies double-ended transfers, and `SequenceClone` supplies endpoint cloning through the shared access view and its generic clone service. Empty outputs retain the concrete element type and clone failures propagate. No deque method names are registered in the macro or execution adapter.

`BinaryHeap<T, S>` uses `SequenceHeap<T>` for priority operations and `SequenceClear` for clearing. Ordering restrictions and heap algorithms remain in the standard library. Its query, cloning, mutation, and owning iterator methods use the same generated signature adapters as other sequences.

Map and Set operations are implemented by standard-library capabilities such as `MapInsert`, `MapSearch`, `MapClone`, `MapKeys`, `MapValues`, `SetInsert`, `SetRelations`, and `SetAlgebra`. Entry construction and extraction follow the declaration schema, including reordered or nested layouts. Two-container operations retain each operand's own services and merge the metadata needed by the result. Set algebra selects identities before cloning and preserves the right operand's value for equal keys. The old `native_collections` helper module and keyed method-name dispatcher have been removed; use `with_storage` and these capabilities for a custom backend.

Static methods returning `Self` can construct a sequence with either no input or one owned array. `SequenceNew` supplies an empty backend. `SequenceFromArray<T>::Array<N>` supplies the array input representation, whose `ArrayInput::build` transfers its elements and wraps the result. A parameter can project its Rust backend type into its Rils declaration:

```rust,ignore
#[export_rils]
#[rils_return(Self)]
pub fn from<const N: usize>(
    #[rils_param([T; N])] values: S::Array<N>,
) -> <S::Array<N> as ArrayInput>::Output<Self> {
    ArrayInput::build(values, Self::with_storage)
}
```

`rils_param` is allowed only on exported inherent method parameters of a storage-backed type; duplicate mappings, receiver mappings, and backend leakage are rejected. Generated source and metadata use the mapped type, while Rust keeps the original signature. Native array construction specializes the erased Rust length to zero; the actual length lives in the owning input cursor. Therefore constructor bodies and where clauses cannot mention that const parameter: query the backend input when length is needed. Other const-dependent constructor signatures are rejected.

The execution boundary resolves the result layout, retains declaration metadata, and validates constructor inputs and outputs. It calls the original method with `OwnedSequence` storage; it does not implement `Vec::from` or bypass a migrated `new` method. Constructors have a typed owned entry point. Rust callers using the default backend may need to specify `Vec::<T>::new()` / `Vec::<T>::from(...)`, `HashMap::<K, V>::new()`, or annotate the result type, because storage is now generic. This applies to all seven collection families.
