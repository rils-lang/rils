//! Typed registration and safe access to erased native storage.

use std::{any::TypeId, cell::RefCell, collections::HashMap, rc::Rc};

use rils_syntax::Type;

use crate::storage::Payload;

type NativeMethod<V> = Rc<dyn for<'a> Fn(&NativeCallContext<'a, V>) -> Result<V, String>>;
type VisitPayload<V> = fn(&NativeObject<V>, &mut dyn FnMut(&V)) -> Result<(), String>;
type StatePayload<V> = fn(&NativeObject<V>) -> Result<bool, String>;

/// Implement this for native containers that retain Rils values or places.
pub trait NativeChildren<V>: 'static {
    fn visit_values(&self, visit: &mut dyn FnMut(&V));

    fn has_active_references(&self) -> bool {
        false
    }

    fn is_partially_moved(&self) -> bool {
        false
    }
}

/// A Rust type's Rils identity and registered operations. The registry does not
/// prescribe any method names or traits.
pub struct NativeType<V> {
    rils_type: Type,
    rust_type: TypeId,
    copy: bool,
    methods: HashMap<String, NativeMethod<V>>,
    visit_payload: Option<VisitPayload<V>>,
    active_references: Option<StatePayload<V>>,
    partially_moved: Option<StatePayload<V>>,
}

impl<V: 'static> NativeType<V> {
    pub fn new<T: 'static>(rils_type: Type) -> Self {
        Self {
            rils_type,
            rust_type: TypeId::of::<T>(),
            copy: false,
            methods: HashMap::new(),
            visit_payload: None,
            active_references: None,
            partially_moved: None,
        }
    }

    /// Mark a Rust `Copy` type as implicitly copyable in Rils. Values that fit
    /// the inline layout are stored without a per-value heap allocation.
    pub fn with_copy<T: Copy + 'static>(mut self) -> Self {
        assert_eq!(self.rust_type, TypeId::of::<T>());
        self.copy = true;
        self
    }

    pub fn register_method(
        mut self,
        name: impl Into<String>,
        method: impl for<'a> Fn(&NativeCallContext<'a, V>) -> Result<V, String> + 'static,
    ) -> Self {
        let name = name.into();
        assert!(
            self.methods.insert(name.clone(), Rc::new(method)).is_none(),
            "native method {name} was registered twice"
        );
        self
    }

    pub fn with_children<T: NativeChildren<V>>(mut self) -> Self {
        assert_eq!(self.rust_type, TypeId::of::<T>());
        self.visit_payload =
            Some(|object, visit| object.with::<T, _>(|payload| payload.visit_values(visit)));
        self.active_references =
            Some(|object| object.with::<T, _>(NativeChildren::has_active_references));
        self.partially_moved =
            Some(|object| object.with::<T, _>(NativeChildren::is_partially_moved));
        self
    }

    pub fn rils_type(&self) -> &Type {
        &self.rils_type
    }

    pub fn is_copy(&self) -> bool {
        self.copy
    }

    pub fn has_method(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }
}

enum NativeStorage {
    Inline(Payload),
    Shared(Rc<RefCell<Payload>>),
}

/// A native value in its Rust layout. Small registered Copy types reside in
/// this handle; all other types retain shared backing for lexical references.
pub struct NativeObject<V> {
    descriptor: Rc<NativeType<V>>,
    storage: NativeStorage,
}

impl<V> Clone for NativeObject<V> {
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            NativeStorage::Inline(payload) => NativeStorage::Inline(payload.copy_value(true)),
            NativeStorage::Shared(payload) => NativeStorage::Shared(payload.clone()),
        };
        Self {
            descriptor: self.descriptor.clone(),
            storage,
        }
    }
}

impl<V: 'static> NativeObject<V> {
    pub fn new<T: 'static>(descriptor: Rc<NativeType<V>>, value: T) -> Result<Self, String> {
        if descriptor.rust_type != TypeId::of::<T>() {
            return Err(format!(
                "native payload does not match descriptor for {}",
                descriptor.rils_type
            ));
        }
        let payload = Payload::new(value, descriptor.copy);
        let storage = if payload.is_inline() {
            NativeStorage::Inline(payload)
        } else {
            NativeStorage::Shared(Rc::new(RefCell::new(payload)))
        };
        Ok(Self {
            descriptor,
            storage,
        })
    }

    pub fn descriptor(&self) -> &NativeType<V> {
        &self.descriptor
    }

    pub fn is_inline(&self) -> bool {
        matches!(self.storage, NativeStorage::Inline(_))
    }

    pub fn with<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, String> {
        self.check_type::<T>()?;
        match &self.storage {
            NativeStorage::Inline(payload) => Ok(payload.with(f)),
            NativeStorage::Shared(payload) => payload
                .try_borrow()
                .map(|payload| payload.with(f))
                .map_err(|_| "native value is already mutably accessed".to_owned()),
        }
    }

    pub fn with_mut<T: 'static, R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, String> {
        self.check_type::<T>()?;
        match &self.storage {
            NativeStorage::Inline(_) => Err(
                "inline Copy value requires mutable place writeback, not a cloned handle".into(),
            ),
            NativeStorage::Shared(payload) => payload
                .try_borrow_mut()
                .map(|mut payload| payload.with_mut(f))
                .map_err(|_| "native value is already accessed".to_owned()),
        }
    }

    /// Copy a value using the registered Rust `Copy` guarantee, including
    /// heap-backed values whose layout exceeds the inline capacity.
    pub fn copy_owned(&self) -> Result<Self, String> {
        if !self.descriptor.copy {
            return Err(format!(
                "native type {} is not Copy",
                self.descriptor.rils_type
            ));
        }
        let payload = match &self.storage {
            NativeStorage::Inline(payload) => payload.copy_value(true),
            NativeStorage::Shared(payload) => payload
                .try_borrow()
                .map_err(|_| "native value is already mutably accessed".to_owned())?
                .copy_value(true),
        };
        let storage = if payload.is_inline() {
            NativeStorage::Inline(payload)
        } else {
            NativeStorage::Shared(Rc::new(RefCell::new(payload)))
        };
        Ok(Self {
            descriptor: self.descriptor.clone(),
            storage,
        })
    }

    pub fn call(&self, name: &str, arguments: &[V]) -> Option<Result<V, String>> {
        let method = self.descriptor.methods.get(name)?;
        Some(method(&NativeCallContext {
            receiver: self,
            arguments,
        }))
    }

    pub fn any_child(&self, mut predicate: impl FnMut(&V) -> bool) -> bool {
        let Some(visit) = self.descriptor.visit_payload else {
            return false;
        };
        let mut found = false;
        if visit(self, &mut |value| found |= predicate(value)).is_err() {
            return true;
        }
        found
    }

    pub fn has_active_references(&self) -> bool {
        self.state_or_conservative(self.descriptor.active_references)
    }

    pub fn is_partially_moved(&self) -> bool {
        self.state_or_conservative(self.descriptor.partially_moved)
    }

    fn state_or_conservative(&self, state: Option<StatePayload<V>>) -> bool {
        state.is_some_and(|state| state(self).unwrap_or(true))
    }

    fn check_type<T: 'static>(&self) -> Result<(), String> {
        if self.descriptor.rust_type == TypeId::of::<T>() {
            Ok(())
        } else {
            Err(format!(
                "native value is not {}",
                std::any::type_name::<T>()
            ))
        }
    }
}

/// The receiver and arguments visible to one registered native operation.
pub struct NativeCallContext<'a, V> {
    receiver: &'a NativeObject<V>,
    arguments: &'a [V],
}

impl<V: 'static> NativeCallContext<'_, V> {
    pub fn arguments(&self) -> &[V] {
        self.arguments
    }

    pub fn receiver<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, String> {
        self.receiver.with(f)
    }

    pub fn receiver_mut<T: 'static, R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, String> {
        self.receiver.with_mut(f)
    }

    pub fn new_object<T: 'static>(&self, value: T) -> Result<NativeObject<V>, String> {
        NativeObject::new(self.receiver.descriptor.clone(), value)
    }
}
