use rils_builtins::{
    BuiltinBackend, BuiltinDeclaration, BuiltinId, BuiltinKind, BuiltinMember, BuiltinMemberKind,
    BuiltinSignature, BuiltinTraitImpl, ReceiverMode, TypePattern,
};
use rils_builtins_macros::{
    decl_rils, decl_rils_metadata, decl_rils_trait_impls, decl_rils_trait_metadata,
};

pub trait Tagged {
    fn tag(&self) -> i32;
}

#[decl_rils(core::fixture)]
mod native {
    #[rils_struct]
    pub struct Sample {
        pub value: i32,
    }

    impl Sample {
        #[export_rils]
        pub fn new() -> Self {
            Self { value: 0 }
        }
    }

    #[rils_enum]
    pub enum Choice {
        None,
        Some(i32),
    }

    impl Choice {
        #[export_rils(native)]
        pub fn is_some(&self) -> bool {
            matches!(self, Self::Some(value) if *value >= 0)
        }
    }

    #[rils_trait]
    pub trait Tagged: super::Tagged {
        fn tag(&self) -> i32;
    }

    #[rils_impl]
    impl Tagged for Sample {
        fn tag(&self) -> i32 {
            self.value
        }
    }

    #[rils_struct]
    pub struct Generic<T> {
        pub value: T,
    }

    #[rils_impl]
    impl<T: Tagged> Tagged for Generic<T> {
        fn tag(&self) -> i32 {
            self.value.tag()
        }
    }
}

mod sample_metadata {
    use super::*;
    macro_rules! builtin_id {
        ("core::fixture::sample::new") => {
            BuiltinId::BinaryHeapNew
        };
    }
    sample_definition!(decl_rils_metadata);
    sample_definition!(decl_rils_trait_impls);
}

mod choice_metadata {
    use super::*;
    choice_definition!(decl_rils_metadata);
}

mod tagged_metadata {
    use super::*;
    tagged_definition!(decl_rils_trait_metadata);
}

mod generic_metadata {
    use super::*;
    generic_definition!(decl_rils_trait_impls);
}

#[test]
fn mixed_module_metadata_tracks_each_export_and_explicit_impl() {
    let sample = &sample_metadata::DECLARATION;
    let choice = &choice_metadata::DECLARATION;
    let tagged = &tagged_metadata::DECLARATION;
    assert_eq!(sample.kind, BuiltinKind::Struct);
    assert_eq!(choice.kind, BuiltinKind::Enum);
    assert_eq!(tagged.kind, BuiltinKind::Trait);
    assert_eq!(
        sample.member("value").unwrap().kind,
        BuiltinMemberKind::Field
    );
    assert_eq!(
        sample.member("new").unwrap().builtin_id,
        Some(BuiltinId::BinaryHeapNew)
    );
    assert_eq!(choice.member("is_some").unwrap().builtin_id, None);
    assert_eq!(
        choice.member("is_some").unwrap().native_symbol,
        Some("core::fixture::choice::is_some")
    );
    assert_eq!(
        tagged.member("tag").unwrap().kind,
        BuiltinMemberKind::Method
    );
    assert_eq!(sample_metadata::TRAIT_IMPLS.len(), 1);
    assert_eq!(sample_metadata::TRAIT_IMPLS[0].trait_name, "Tagged");
    assert_eq!(
        generic_metadata::TRAIT_IMPLS[0].requirements,
        &[("T", "Tagged")]
    );
    assert_eq!(Tagged::tag(&native::Sample::new()), 0);
    assert_eq!(
        Tagged::tag(&native::Generic {
            value: native::Sample::new()
        }),
        0
    );
    assert!(!native::Choice::None.is_some());
    assert!(native::Choice::Some(1).is_some());
}
