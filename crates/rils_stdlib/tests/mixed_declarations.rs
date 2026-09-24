use rils_builtins_macros::{
    decl_rils, decl_rils_source, decl_rils_trait_impls, decl_rils_trait_source,
};

pub struct BuiltinTraitImpl {
    pub type_name: &'static str,
    pub trait_name: &'static str,
    pub requirements: &'static [(&'static str, &'static str)],
}

pub trait Label {
    fn label(&self) -> i32;
}

#[decl_rils(core::mixed_fixture)]
mod native {
    #[rils_struct]
    pub struct Counter {
        pub value: i32,
        hidden: i32,
    }

    impl Counter {
        #[export_rils]
        pub fn new(value: i32) -> Self {
            Self { value, hidden: 1 }
        }

        pub fn hidden(&self) -> i32 {
            self.hidden
        }
    }

    #[rils_enum]
    pub enum State {
        Idle,
        Running(i32),
    }

    impl State {
        #[export_rils]
        pub fn is_running(&self) -> bool {
            matches!(self, Self::Running(value) if *value >= 0)
        }
    }

    #[rils_trait]
    pub trait Label: super::Label {
        fn label(&self) -> i32;
    }

    #[rils_impl]
    impl Label for Counter {
        fn label(&self) -> i32 {
            self.value
        }
    }

    #[rils_struct]
    pub struct Plain;

    #[rils_impl]
    impl Label for Plain {
        fn label(&self) -> i32 {
            0
        }
    }

    #[rils_struct]
    pub struct Unmarked;

    impl Label for Unmarked {
        fn label(&self) -> i32 {
            -1
        }
    }

    #[rils_derive(Label)]
    fn derive_label(
        _: &rils_syntax::ast::Stmt,
    ) -> Result<Option<rils_syntax::quote::QuotedStatement>, rils_syntax::parser::ParseError> {
        Ok(None)
    }

    pub struct Helper;
}

mod counter_impls {
    use super::*;
    counter_definition!(decl_rils_trait_impls);
}

mod unmarked_impls {
    use super::*;
    unmarked_definition!(decl_rils_trait_impls);
}

const COMBINED_SOURCE: &str = concat!(
    label_definition!(decl_rils_trait_source),
    counter_definition!(decl_rils_source),
    state_definition!(decl_rils_source),
    plain_definition!(decl_rils_source),
);

#[test]
fn mixed_module_exports_only_marked_types_methods_and_impls() {
    let counter = counter_definition!(decl_rils_source);
    let state = state_definition!(decl_rils_source);
    let label = label_definition!(decl_rils_trait_source);
    let plain = plain_definition!(decl_rils_source);
    assert!(counter.contains("pub struct Counter {"));
    assert!(counter.contains("value: i32"));
    assert!(!counter.contains("hidden:"));
    assert!(counter.contains("fn new"));
    assert!(!counter.contains("fn hidden"));
    assert!(state.contains("pub enum State"));
    assert!(state.contains("fn is_running"));
    assert!(label.contains("pub trait Label"));
    assert!(plain.contains("pub struct Plain;"));
    let tokens = rils_syntax::lex(COMBINED_SOURCE).unwrap();
    rils_syntax::parser::parse_builtin_declarations(tokens).unwrap();
    assert_eq!(counter_impls::TRAIT_IMPLS.len(), 1);
    assert_eq!(counter_impls::TRAIT_IMPLS[0].trait_name, "Label");
    assert!(unmarked_impls::TRAIT_IMPLS.is_empty());
    assert_eq!(native::DERIVE_LABEL.name, "Label");
    let counter = native::Counter::new(41);
    assert_eq!(Label::label(&counter), 41);
    assert_eq!(counter.hidden(), 1);
    assert_eq!(Label::label(&native::Plain), 0);
    assert_eq!(Label::label(&native::Unmarked), -1);
    assert!(native::State::Running(1).is_running());
    assert!(!native::State::Idle.is_running());
    let _ = native::Helper;
}
