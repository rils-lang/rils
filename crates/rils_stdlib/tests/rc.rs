use rils_stdlib::stdlib::{
    option::Option,
    rc::{Rc, Weak},
};

#[test]
fn native_shared_handles_preserve_ownership_counts_and_upgrade() {
    let owner = Rc::new(42);
    let weak = owner.downgrade();
    let copy = Clone::clone(&owner);
    assert_eq!(owner.strong_count(), 2);
    assert_eq!(weak.strong_count(), 2);
    assert_eq!(weak.weak_count(), 1);
    assert!(matches!(weak.upgrade(), Option::Some(_)));
    drop(owner);
    drop(copy);
    assert!(matches!(weak.upgrade(), Option::None));
}

#[test]
fn shared_wrappers_deref_to_their_std_handles() {
    let mut owner = Rc::from(std::rc::Rc::new(1));
    assert_eq!(**owner, 1);
    *owner = std::rc::Rc::new(2);
    assert_eq!(**owner, 2);

    let mut weak = owner.downgrade();
    assert_eq!(std::rc::Weak::strong_count(&*weak), 1);
    *weak = std::rc::Weak::new();
    assert!(matches!(weak.upgrade(), Option::None));

    let _: std::rc::Rc<i32> = owner.into();
    let _: std::rc::Weak<i32> = Weak::from(std::rc::Weak::new()).into();
}
