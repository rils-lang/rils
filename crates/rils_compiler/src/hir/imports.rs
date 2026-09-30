//! Collection constructor symbols used by default lowering.

pub(super) fn collection_constructor_symbol(name: &str) -> Option<&'static str> {
    let owner = name.rsplit("::").nth(1)?;
    rils_builtins::builtin_member(owner, "new")?.native_symbol
}
