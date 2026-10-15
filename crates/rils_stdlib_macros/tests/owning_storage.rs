extern crate self as owning_fixture;
extern crate self as rils_stdlib;
extern crate self as rils_value;
#[path = "support/storage_schema.rs"]
pub mod storage;
#[path = "fixtures/owning_storage.rs"]
#[macro_use]
mod fixture;
pub use fixture::Buffer;

#[derive(Debug, PartialEq)]
pub struct DynamicValue(usize);
#[derive(Debug, PartialEq)]
pub struct Value(Vec<usize>);
mod runtime_builtins {
    pub struct NativeOwnedContext;
}
mod sequence_access {
    use super::*;
    pub fn with_owned_sequence(
        mut arguments: Vec<Value>,
        schema: &storage::EntrySchema,
        _: &runtime_builtins::NativeOwnedContext,
        call: impl FnOnce(
            std::vec::IntoIter<Result<DynamicValue, String>>,
        ) -> std::vec::IntoIter<Result<DynamicValue, String>>,
    ) -> Result<Value, String> {
        assert_eq!(schema.owner, "Buffer");
        let values = arguments
            .pop()
            .unwrap()
            .0
            .into_iter()
            .map(|v| Ok(DynamicValue(v)))
            .collect::<Vec<_>>();
        call(values.into_iter())
            .map(|v| v.map(|v| v.0))
            .collect::<Result<Vec<_>, _>>()
            .map(Value)
    }
}
mod generated {
    pub mod buffer {
        use rils_stdlib_macros::decl_rils_native;
        buffer_definition!(decl_rils_native);
    }
}
#[test]
fn owning_trait_body_transforms_the_original_cursor_on_both_backends() {
    let symbol = "core::fixture::buffer::into_iter";
    assert!(generated::buffer::is_owned_symbol(symbol));
    for items in [vec![], vec![7], vec![1, 2, 3]] {
        let expected = Buffer::<usize>::with_storage(items.clone())
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(
            generated::buffer::call_owned_symbol(
                symbol,
                vec![Value(items)],
                &runtime_builtins::NativeOwnedContext
            ),
            Some(Ok(Value(expected)))
        );
    }
    assert!(
        generated::buffer::call_owned_symbol(symbol, vec![], &runtime_builtins::NativeOwnedContext)
            .unwrap()
            .is_err()
    );
    assert!(
        generated::buffer::call_symbol(symbol, &[Value(vec![1])])
            .unwrap()
            .is_err()
    );
}
