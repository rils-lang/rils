use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    ast::{
        AssociatedType, Block, EnumVariant, GenericParameter, NamedField, Parameter, TraitMethod,
    },
    environment::{EnvironmentRef, StorageRef},
    types::{FunctionSignature, Type},
};

#[path = "value/display.rs"]
mod display;

#[path = "value/range.rs"]
mod range;
pub use range::native_range;

pub use crate::declaration::{Binding, Declaration};
pub use rils_value::NativeChildren;
pub type NativeObject = rils_value::NativeObject<Value>;
pub type NativeType = rils_value::NativeType<Value>;
pub type DynamicObject = rils_value::DynamicObject<Value>;
pub mod borrowed_sum;
pub mod callable;
#[path = "value/dynamic_option.rs"]
pub mod dynamic_option;
pub mod dynamic_result;
pub mod dynamic_sequence;
pub mod host_declarations;
pub(crate) mod native_array;
pub mod native_instance;
#[path = "value/native_layouts.rs"]
pub mod native_layouts;
pub mod native_ops;
pub(crate) mod native_receiver;
#[path = "value/record_codec.rs"]
pub mod record_codec;
#[path = "value/record_layout.rs"]
pub mod record_layout;
pub mod runtime_layouts;
#[path = "value/string.rs"]
mod string;
pub mod sum;
pub use string::{native_string, string_payload};
#[path = "value/character.rs"]
mod character;
pub use character::{char_payload, native_char};
mod access;
pub(crate) mod borrowed;
mod conversion;
pub mod equality;
mod ownership;
mod primitive;
pub use crate::numeric::{
    native_f32, native_f64, native_i8, native_i16, native_i32, native_i64, native_i128,
    native_isize, native_u8, native_u16, native_u32, native_u64, native_u128, native_usize,
};
pub use primitive::{native_bool, native_unit};

pub mod storage;

#[path = "value/reference.rs"]
mod reference;
pub use reference::ReferenceValue;

#[path = "value/iterator.rs"]
mod iterator;
pub(crate) use iterator::BorrowedIteratorProjection;
pub use iterator::{BorrowedIndexedIteratorValue, IndexedIteratorStorage, OwnedIteratorValue};

pub type HostFunctionHandler = dyn Fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub struct UserFunction {
    pub name: String,
    pub generic_parameters: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub closure: EnvironmentRef,
    pub semantic_expression_ids: Option<rils_frontend::semantic::ExpressionIdentityMap>,
    pub typeck_results: Option<Rc<rils_frontend::TypeckResults>>,
}

#[derive(Clone)]
pub struct BytecodeFunctionValue {
    pub function: usize,
    pub name: String,
    pub parameter_count: usize,
    pub signature: Type,
    pub captures: Vec<StorageRef>,
    pub bound_arguments: Vec<Value>,
    /// Concrete lexical type parameters retained by a returned closure.
    pub type_bindings: HashMap<String, Type>,
}

#[derive(Clone)]
pub struct BytecodeIteratorValue {
    pub storage: StorageRef,
    pub next_function: usize,
}

#[derive(Clone)]
pub struct NativeFunction {
    pub binding_name: &'static str,
    pub name: &'static str,
    pub min_arity: usize,
    pub max_arity: usize,
    pub signature: Option<FunctionSignature>,
    pub body: NativeFunctionBody,
}

#[derive(Clone, Copy)]
pub enum NativeFunctionBody {
    Rust(fn(&[Value]) -> Result<Value, String>),
    RustOwned(NativeOwnedFunction),
    Symbol(&'static str),
}

/// An owned native call receives visible declarations and the checked result
/// type when invoked from a typed expression.
pub type NativeOwnedFunction = fn(
    Vec<Value>,
    &crate::runtime_builtins::NativeOwnedContext,
    Option<&Type>,
) -> Result<Value, String>;

#[derive(Clone)]
pub struct HostFunction {
    pub name: String,
    pub min_arity: usize,
    pub max_arity: usize,
    pub signature: Option<FunctionSignature>,
    pub function: Rc<HostFunctionHandler>,
}

pub struct HostType {
    pub name: String,
    pub base_types: HashSet<String>,
    pub copy: bool,
    pub methods: RefCell<HashMap<String, Rc<HostFunction>>>,
}

#[derive(Clone)]
pub struct HostObject {
    pub type_definition: Rc<HostType>,
    pub payload: Rc<dyn Any>,
}

#[derive(Clone)]
pub struct HostBoundMethod {
    pub receiver: Rc<Value>,
    pub function: Rc<HostFunction>,
}

pub struct StructType {
    pub name: String,
    pub opaque_native: bool,
    pub generic_parameters: Vec<GenericParameter>,
    pub fields: Vec<NamedField>,
    pub field_indices: std::cell::OnceCell<HashMap<String, usize>>,
    pub methods: RefCell<HashMap<String, Rc<UserFunction>>>,
    pub trait_methods: RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    pub implemented_traits: RefCell<HashSet<String>>,
    pub associated_types: RefCell<HashMap<String, HashMap<String, TypeAliasType>>>,
}

impl StructType {
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.field_indices
            .get_or_init(|| {
                self.fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| (field.name.clone(), index))
                    .collect()
            })
            .get(name)
            .copied()
    }
}

#[derive(Clone)]
pub struct EnumType {
    pub name: String,
    /// Portable host identity; script declarations use their registered object.
    pub host_definition: Option<rils_host::HostEnumDefinition>,
    pub generic_parameters: Vec<GenericParameter>,
    pub variants: Vec<EnumVariant>,
    pub methods: RefCell<HashMap<String, Rc<UserFunction>>>,
    pub trait_methods: RefCell<HashMap<String, HashMap<String, Rc<UserFunction>>>>,
    pub implemented_traits: RefCell<HashSet<String>>,
    pub associated_types: RefCell<HashMap<String, HashMap<String, TypeAliasType>>>,
}

pub struct TraitType {
    pub name: String,
    pub generic_parameters: Vec<crate::ast::GenericParameter>,
    pub bounds: Vec<String>,
    pub associated_types: Vec<AssociatedType>,
    pub methods: Vec<TraitMethod>,
}

pub struct ModuleValue {
    pub name: String,
    pub members: EnvironmentRef,
    pub public: RefCell<HashSet<String>>,
}

#[derive(Clone)]
pub struct TypeAliasType {
    pub name: String,
    pub generic_parameters: Vec<GenericParameter>,
    pub target: Type,
}

#[derive(Clone)]
pub struct VariantConstructor {
    pub type_definition: Rc<EnumType>,
    pub variant: String,
    pub environment: EnvironmentRef,
}

#[derive(Clone)]
pub struct BoundMethod {
    pub receiver: Rc<Value>,
    pub function: Rc<UserFunction>,
}

#[derive(Clone, Copy)]
pub enum BuiltinMethod {
    Native(&'static str),
    IteratorIdentity,
}

#[derive(Clone, Copy)]
pub enum BuiltinType {
    Integer(crate::IntegerType),
    Float(crate::FloatType),
}

#[derive(Clone, Copy)]
pub enum BuiltinFunction {
    IntegerIntrinsic {
        symbol: &'static str,
        target: crate::IntegerType,
    },
}

#[derive(Clone)]
pub struct BuiltinBoundMethod {
    pub receiver: Rc<Value>,
    pub method: BuiltinMethod,
}

#[derive(Clone)]
pub struct TraitMethodSelector {
    pub target: Option<Type>,
    pub trait_name: String,
    pub method_name: String,
    pub environment: EnvironmentRef,
}

#[derive(Clone)]
pub enum Value {
    HostObject(Rc<HostObject>),
    Native(NativeObject),
    Dynamic(DynamicObject),
}

impl Value {
    pub fn type_name(&self) -> String {
        match self {
            __reference_value if __reference_value.as_reference().is_some() => {
                let reference = __reference_value
                    .as_reference()
                    .expect("checked reference handle");
                {
                    let name = match reference.referent_type() {
                        Ok(ty) => ty.to_string(),
                        Err(_) => return "invalid reference".into(),
                    };
                    if reference.mutable {
                        format!("&mut {name}")
                    } else {
                        format!("&{name}")
                    }
                }
            }

            Self::HostObject(object) => object.type_definition.name.clone(),
            Self::Native(object) => object.descriptor().rils_type().to_string(),
            Self::Dynamic(object) => object.descriptor().layout().rils_type().to_string(),
        }
    }

    pub fn is_truthy(&self) -> bool {
        !self.is_unit() && self.get_cloned::<bool>().ok() != Some(false)
    }

    pub fn host_payload<T: 'static>(&self) -> Option<&T> {
        let Self::HostObject(object) = self else {
            return None;
        };
        object.payload.downcast_ref::<T>()
    }
}

pub fn enum_variant_name(variant: &EnumVariant) -> &str {
    match variant {
        EnumVariant::Unit { name, .. }
        | EnumVariant::Tuple { name, .. }
        | EnumVariant::Record { name, .. } => name,
    }
}
