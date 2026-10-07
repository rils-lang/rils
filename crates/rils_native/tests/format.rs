use std::{cell::Cell, fmt, rc::Rc};

use rils_native::{FormatRegistration, NativeRegistry};
use rils_syntax::Type;
use rils_value::{DynamicLayout, DynamicValue};

struct Probe(Rc<Cell<usize>>);

impl fmt::Display for Probe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("display")
    }
}

impl fmt::Debug for Probe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if formatter.alternate() {
            "alternate"
        } else {
            "debug"
        })
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

static REGISTRY: NativeRegistry =
    NativeRegistry::new(&[], &[]).with_formats(&[FormatRegistration::of::<Probe>()]);

struct Render<'a>(&'a DynamicValue);

impl fmt::Display for Render<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        REGISTRY
            .format_view(&self.0.view(), formatter, false, no_child)
            .unwrap()
            .unwrap()
    }
}

impl fmt::Debug for Render<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        REGISTRY
            .format_view(&self.0.view(), formatter, true, no_child)
            .unwrap()
            .unwrap()
    }
}

#[test]
fn formatting_keeps_non_clone_payloads_alive_and_preserves_formatter_flags() {
    let drops = Rc::new(Cell::new(0));
    let value = DynamicValue::from_rust(
        DynamicLayout::of::<Probe>(Type::named("Probe")),
        Probe(drops.clone()),
    )
    .unwrap();
    assert_eq!(format!("{}", Render(&value)), "display");
    assert_eq!(format!("{:?}", Render(&value)), "debug");
    assert_eq!(format!("{:#?}", Render(&value)), "alternate");
    assert_eq!(drops.get(), 0);
    drop(value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn registration_checks_physical_type_even_when_rils_names_match() {
    let layout = DynamicLayout::copy_of::<i32>(Type::named("Probe"));
    assert!(!(FormatRegistration::of::<Probe>().matches)(&layout));
}

fn no_child(
    _: rils_value::DynamicValueRef<'_>,
    _: &mut fmt::Formatter<'_>,
    _: bool,
) -> fmt::Result {
    unreachable!("leaf has no children")
}
