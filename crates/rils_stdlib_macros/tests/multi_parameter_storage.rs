extern crate self as rils_value;
extern crate self as sequence_fixture;

pub use rils_syntax::Type;
use std::rc::Rc;

#[path = "fixtures/multi_parameter_storage.rs"]
#[macro_use]
mod fixture;
pub use fixture::Reordered;

// A layout recorder verifies generated resolution and projection without relying
// on execution's collection implementation or allocating collection values.
#[derive(Debug, PartialEq)]
pub struct DynamicLayout {
    ty: Type,
    item: Option<Rc<Self>>,
    fields: Vec<(String, Rc<Self>)>,
}
impl DynamicLayout {
    fn leaf(ty: Type) -> Rc<Self> {
        Rc::new(Self {
            ty,
            item: None,
            fields: vec![],
        })
    }
    pub fn aggregate(ty: Type, fields: Vec<(String, Rc<Self>)>) -> Result<Rc<Self>, String> {
        Ok(Rc::new(Self {
            ty,
            item: None,
            fields,
        }))
    }
    pub fn sequence(ty: Type, item: Rc<Self>) -> Rc<Self> {
        Rc::new(Self {
            ty,
            item: Some(item),
            fields: vec![],
        })
    }
}

mod layout {
    use rils_stdlib_macros::decl_rils_layout;
    reordered_definition!(decl_rils_layout);
}

macro_rules! source {
    ($($tokens:tt)*) => {
        const SOURCE: &str = rils_stdlib_macros::decl_rils_source! { $($tokens)* };
    };
}
reordered_definition!(source);

#[test]
fn reordered_entry_layout_follows_declared_parameters_and_preserves_tuple_fields() {
    let ty = Type::Named {
        name: "Reordered".into(),
        arguments: vec![Type::I32, Type::Bool],
    };
    let mut resolved = vec![];
    let value = layout::layout(&ty, &mut |ty| {
        resolved.push(ty.clone());
        Ok(DynamicLayout::leaf(ty.clone()))
    })
    .unwrap()
    .unwrap();
    assert_eq!(resolved, [Type::Bool, Type::I32]);
    assert_eq!(value.ty, ty);
    let item = value.item.as_ref().unwrap();
    assert_eq!(item.ty, Type::Tuple(vec![Type::Bool, Type::I32]));
    assert_eq!(
        item.fields
            .iter()
            .map(|(name, ty)| (name.as_str(), ty.ty.clone()))
            .collect::<Vec<_>>(),
        vec![("0", Type::Bool), ("1", Type::I32)]
    );
    let values = Reordered::<i32, bool>::from(vec![(true, 7), (false, 9)]);
    assert_eq!(
        values.into_iter().collect::<Vec<_>>(),
        [(true, 7), (false, 9)]
    );
    let source = SOURCE.replace(' ', "");
    assert!(source.contains("pubstructReordered<A,B>"));
    assert!(source.contains("typeItem=(B,A)"));
    assert!(source.contains("typeIntoIter=Entries<A,B>"));
    assert!(!SOURCE.contains("S::IntoIter"));
    rils_syntax::parser::parse_builtin_declarations(rils_syntax::lex(SOURCE).unwrap()).unwrap();
}

#[test]
fn layout_resolution_checks_nominal_arity_and_propagates_child_failure() {
    for ty in [
        Type::Named {
            name: "Other".into(),
            arguments: vec![Type::I32, Type::Bool],
        },
        Type::Named {
            name: "Reordered".into(),
            arguments: vec![Type::I32],
        },
    ] {
        assert!(!layout::matches(&ty));
        assert!(
            layout::layout(&ty, &mut |_| panic!("invalid receiver reached resolver")).is_none()
        );
    }
    let ty = Type::Named {
        name: "Reordered".into(),
        arguments: vec![Type::I32, Type::Bool],
    };
    let mut resolved = vec![];
    let error = layout::layout(&ty, &mut |ty| {
        resolved.push(ty.clone());
        Err("child layout rejected".into())
    })
    .unwrap()
    .unwrap_err();
    assert_eq!(error, "child layout rejected");
    assert_eq!(resolved, [Type::Bool]);
}
