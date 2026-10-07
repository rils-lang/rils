mod patterns;
mod records;

use std::collections::{HashMap, HashSet};

use crate::{
    ExprId, SourceId,
    ast::{
        BinaryOp, Block, EnumVariant, Expr, Literal, Parameter, Pattern, Program, Stmt, UnaryOp,
    },
    source::Span,
    types::{FunctionSignature, Type, merge_types},
};

mod calls;
mod constructors;
mod expected;
mod expressions;
mod numeric;
mod statements;

#[derive(Clone, Debug)]
pub(crate) struct RawTypeHint {
    pub position: usize,
    pub span: Span,
    pub ty: Type,
    pub prefix: &'static str,
}

#[derive(Default)]
pub(crate) struct InferenceResult {
    pub binding_types: HashMap<Span, Type>,
    pub expression_types_by_id: HashMap<ExprId, Type>,
    pub sum_constructors: HashMap<ExprId, &'static str>,
    pub hints: Vec<RawTypeHint>,
    pub expression_ids: crate::semantic::ExpressionIdentityMap,
}

#[derive(Clone)]
struct Binding {
    ty: Type,
    constructor: Option<&'static str>,
}

#[derive(Clone, Default)]
struct TypeDefinition {
    generic_parameters: Vec<String>,
    fields: HashMap<String, Type>,
    variants: HashMap<String, VariantDefinition>,
    methods: HashMap<String, Type>,
    method_receivers: HashMap<String, Type>,
    implemented_traits: HashSet<String>,
    associated_types: HashMap<(String, String), Type>,
}

#[derive(Clone)]
enum VariantDefinition {
    Unit,
    Tuple(Vec<Type>),
    Record(HashMap<String, Type>),
}

fn qualified_type_name(prefix: &[String], name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", prefix.join("::"))
    }
}

pub(crate) fn infer_with_host_functions_and_host_types(
    program: &Program,
    source: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
    host_type_resolutions: &crate::HostTypeResolutionResults,
    host_contract: Option<&rils_host::HostContract>,
    declarations: (&crate::semantic::DeclarationTypeResolver, &[String]),
) -> InferenceResult {
    Inferencer::new(
        program,
        source,
        host_functions,
        host_type_resolutions,
        host_contract,
        declarations,
    )
    .run(program)
}

struct Inferencer<'a> {
    scopes: Vec<HashMap<String, Binding>>,
    types: HashMap<String, TypeDefinition>,
    variant_owners: HashMap<String, String>,
    result: InferenceResult,
    expression_ids: crate::semantic::ExpressionIdentityMap,
    numeric_parents: HashMap<ExprId, ExprId>,
    numeric_fixed: HashMap<ExprId, Type>,
    host_functions: HashMap<String, FunctionSignature>,
    host_types: crate::HostTypeResolutionView<'a>,
    declaration_types: crate::semantic::DeclarationTypeResolver,
    module_path: Vec<String>,
}

impl<'a> Inferencer<'a> {
    fn new(
        program: &Program,
        source: SourceId,
        host_functions: &HashMap<String, FunctionSignature>,
        host_type_resolutions: &'a crate::HostTypeResolutionResults,
        host_contract: Option<&rils_host::HostContract>,
        declarations: (&crate::semantic::DeclarationTypeResolver, &[String]),
    ) -> Self {
        let mut globals = HashMap::new();
        for (name, return_type) in [
            ("#rils_native_print", Type::Unit),
            ("#rils_native_println", Type::Unit),
            ("type_of", Type::String),
            ("clone", Type::Unknown),
            ("#rils_native_assert", Type::Unit),
            ("is_some", Type::Bool),
            ("is_none", Type::Bool),
        ] {
            globals.insert(
                name.into(),
                Binding {
                    constructor: None,
                    ty: Type::Function {
                        parameters: None,
                        return_type: Box::new(return_type),
                    },
                },
            );
        }
        for builtin in rils_builtins::BUILTINS.iter().filter(|builtin| {
            builtin.kind == rils_builtins::BuiltinKind::Function && !builtin.path.contains("::")
        }) {
            if let Some(signature) =
                crate::standard_library::standard_function_signature(builtin.path)
            {
                globals.insert(
                    builtin.path.into(),
                    Binding {
                        constructor: constructors::standard_constructor(builtin.path),
                        ty: signature.as_type(),
                    },
                );
            }
        }
        for (name, signature) in host_functions {
            if !name.contains("::") {
                globals.insert(
                    name.clone(),
                    Binding {
                        constructor: None,
                        ty: signature.as_type(),
                    },
                );
            }
        }
        for integer in crate::types::IntegerType::ALL {
            globals.insert(
                integer.name().into(),
                Binding {
                    constructor: None,
                    ty: Type::Integer(integer),
                },
            );
        }
        for float in [crate::types::FloatType::F32, crate::types::FloatType::F64] {
            globals.insert(
                float.name().into(),
                Binding {
                    constructor: None,
                    ty: Type::Float(float),
                },
            );
        }

        let expression_ids = crate::semantic::ExpressionIdentityMap::allocate(program, source);
        let mut inferencer = Self {
            scopes: vec![globals],
            types: HashMap::new(),
            variant_owners: HashMap::new(),
            result: InferenceResult {
                binding_types: HashMap::new(),
                expression_types_by_id: HashMap::new(),
                sum_constructors: HashMap::new(),
                hints: Vec::new(),
                expression_ids: crate::semantic::ExpressionIdentityMap::default(),
            },
            expression_ids,
            numeric_parents: HashMap::new(),
            numeric_fixed: HashMap::new(),
            host_functions: host_functions.clone(),
            declaration_types: declarations.0.clone(),
            module_path: declarations.1.to_vec(),
            host_types: crate::HostTypeResolutionView::new(program, source, host_type_resolutions),
        };
        inferencer.collect_host_type_definitions(host_contract);
        inferencer.collect_type_definitions(&program.statements, &mut declarations.1.to_vec());
        inferencer
    }

    fn collect_host_type_definitions(&mut self, host: Option<&rils_host::HostContract>) {
        let Some(host) = host else {
            return;
        };
        for declaration in host.types() {
            let Some(host_enum) = declaration.enum_definition.as_ref() else {
                continue;
            };
            let mut definition = TypeDefinition::default();
            definition.variants.extend(
                host_enum
                    .variants
                    .keys()
                    .cloned()
                    .map(|variant| (variant, VariantDefinition::Unit)),
            );
            if host_enum.flags {
                definition.implemented_traits.insert("BitFlags".into());
            }
            for variant in host_enum.variants.keys() {
                self.variant_owners
                    .insert(variant.clone(), declaration.name.clone());
            }
            self.types.insert(declaration.name.clone(), definition);
        }
    }

    fn syntax_type(&self, ty: &Type) -> Type {
        let ty = self
            .declaration_types
            .resolve(&self.host_types.resolved_type(ty), &self.module_path);
        if let Type::Associated {
            base,
            trait_name: Some(trait_name),
            name,
            arguments,
        } = &ty
            && trait_name == "IntoIterator"
            && arguments.is_empty()
            && let Type::Named {
                name: owner,
                arguments: type_arguments,
            } = base.as_ref()
        {
            if name == "Item" {
                if let Some(definition) = self.types.get(owner) {
                    let substitutions = definition
                        .generic_parameters
                        .iter()
                        .cloned()
                        .zip(type_arguments.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    if let Some(item) = definition
                        .associated_types
                        .get(&(String::from("IntoIterator"), String::from("Item")))
                        .or_else(|| {
                            definition
                                .associated_types
                                .get(&(String::from("Iterator"), String::from("Item")))
                        })
                    {
                        return item.substitute(&substitutions);
                    }
                }
                let item = self.iterable_item_type(base);
                if item != Type::Unknown {
                    return item;
                }
            }
            if name == "IntoIter"
                && (self
                    .types
                    .get(owner)
                    .is_some_and(|definition| definition.implemented_traits.contains("Iterator"))
                    || rils_builtins::builtin(owner)
                        .is_some_and(|definition| definition.member("next").is_some()))
            {
                return *base.clone();
            }
        }
        ty
    }

    fn optional_syntax_type(&self, ty: Option<&Type>) -> Type {
        ty.map_or(Type::Unknown, |ty| self.syntax_type(ty))
    }

    fn impl_parameter_type(&self, parameter: &Parameter, target: &Type) -> Type {
        if parameter.name != "self" {
            return self.optional_syntax_type(parameter.type_annotation.as_ref());
        }
        let Some(ty) = parameter.type_annotation.as_ref() else {
            return target.clone();
        };
        resolve_impl_self(&self.syntax_type(ty), target)
    }

    fn run(mut self, program: &Program) -> InferenceResult {
        let mut returns = Vec::new();
        self.statements(&program.statements, &mut returns);
        let binding_spans = self
            .result
            .binding_types
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for span in binding_spans {
            if let Some(ty) = self.result.binding_types.get(&span).cloned() {
                let resolved = self.resolve_type(&ty);
                self.result.binding_types.insert(span, resolved);
            }
        }
        let expression_ids = self
            .result
            .expression_types_by_id
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in expression_ids {
            if let Some(ty) = self.result.expression_types_by_id.get(&id).cloned() {
                let resolved = self.resolve_type(&ty);
                self.result.expression_types_by_id.insert(id, resolved);
            }
        }
        for index in 0..self.result.hints.len() {
            let ty = self.result.hints[index].ty.clone();
            self.result.hints[index].ty = self.resolve_type(&ty);
        }
        self.result.expression_ids = self.expression_ids;
        self.result
    }

    fn collect_type_definitions(&mut self, statements: &[Stmt], prefix: &mut Vec<String>) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    name,
                    statements: Some(statements),
                    ..
                } => {
                    prefix.push(name.clone());
                    self.collect_type_definitions(statements, prefix);
                    prefix.pop();
                }
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => {
                    let definition = TypeDefinition {
                        generic_parameters: generic_parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        fields: fields
                            .iter()
                            .map(|field| {
                                (
                                    field.name.clone(),
                                    self.declaration_types.resolve(
                                        &self.host_types.resolved_type(&field.type_annotation),
                                        prefix,
                                    ),
                                )
                            })
                            .collect(),
                        variants: HashMap::new(),
                        methods: HashMap::new(),
                        method_receivers: HashMap::new(),
                        implemented_traits: HashSet::new(),
                        associated_types: HashMap::new(),
                    };
                    let qualified = qualified_type_name(prefix, name);
                    self.types.insert(qualified, definition.clone());
                    self.types.entry(name.clone()).or_insert(definition);
                }
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => {
                    let mut definition = TypeDefinition {
                        generic_parameters: generic_parameters
                            .iter()
                            .map(|parameter| parameter.name.clone())
                            .collect(),
                        ..TypeDefinition::default()
                    };
                    for variant in variants {
                        let (variant_name, payload) = match variant {
                            EnumVariant::Unit { name, .. } => (name, VariantDefinition::Unit),
                            EnumVariant::Tuple { name, fields, .. } => (
                                name,
                                VariantDefinition::Tuple(
                                    fields.iter().map(|field| self.syntax_type(field)).collect(),
                                ),
                            ),
                            EnumVariant::Record { name, fields, .. } => (
                                name,
                                VariantDefinition::Record(
                                    fields
                                        .iter()
                                        .map(|field| {
                                            (
                                                field.name.clone(),
                                                self.syntax_type(&field.type_annotation),
                                            )
                                        })
                                        .collect(),
                                ),
                            ),
                        };
                        definition.variants.insert(variant_name.clone(), payload);
                        self.variant_owners
                            .insert(variant_name.clone(), qualified_type_name(prefix, name));
                    }
                    let qualified = qualified_type_name(prefix, name);
                    self.types.insert(qualified, definition.clone());
                    self.types.entry(name.clone()).or_insert(definition);
                }
                Stmt::Impl {
                    target,
                    trait_name,
                    associated_types,
                    methods,
                    ..
                } => {
                    let target = self.syntax_type(target);
                    let Type::Named { name, .. } = &target else {
                        continue;
                    };
                    let Some(definition) = self.types.get_mut(name) else {
                        continue;
                    };
                    if let Some(trait_name) = trait_name {
                        definition.implemented_traits.insert(trait_name.clone());
                        for associated in associated_types {
                            if let Some(value) = &associated.value {
                                definition.associated_types.insert(
                                    (trait_name.clone(), associated.name.clone()),
                                    self.host_types.resolved_type(value),
                                );
                            }
                        }
                    }
                    for method in methods {
                        let receiver = method
                            .parameters
                            .first()
                            .filter(|parameter| parameter.name == "self")
                            .map(|parameter| {
                                parameter.type_annotation.as_ref().map_or_else(
                                    || target.clone(),
                                    |ty| {
                                        resolve_impl_self(
                                            &self.host_types.resolved_type(ty),
                                            &target,
                                        )
                                    },
                                )
                            });
                        let parameters = method
                            .parameters
                            .iter()
                            .filter(|parameter| parameter.name != "self")
                            .map(|parameter| {
                                parameter
                                    .type_annotation
                                    .as_ref()
                                    .map_or(Type::Unknown, |ty| {
                                        resolve_impl_self(
                                            &self.host_types.resolved_type(ty),
                                            &target,
                                        )
                                    })
                            })
                            .collect();
                        let function = Type::function(
                            parameters,
                            method.return_type.as_ref().map_or(Type::Unknown, |ty| {
                                resolve_impl_self(&self.host_types.resolved_type(ty), &target)
                            }),
                        );
                        if trait_name.is_none() || !definition.methods.contains_key(&method.name) {
                            definition.methods.insert(method.name.clone(), function);
                            if let Some(receiver) = receiver {
                                definition
                                    .method_receivers
                                    .insert(method.name.clone(), receiver);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn field_type(&self, object_type: &Type, field: &str) -> Type {
        if let Type::Reference { inner, .. } = object_type {
            return self.field_type(inner, field);
        }
        if let Type::Integer(integer) = object_type
            && let Some(intrinsic) = rils_builtins::integer_method(field)
        {
            return crate::standard_library::integer_intrinsic_type(intrinsic, *integer);
        }
        if let Type::Float(float) = object_type
            && let Some(intrinsic) = rils_builtins::float_method(field)
        {
            return crate::standard_library::float_intrinsic_type(intrinsic, *float);
        }
        if let Type::Tuple(elements) = object_type {
            return field
                .parse::<usize>()
                .ok()
                .and_then(|index| elements.get(index))
                .cloned()
                .unwrap_or(Type::Unknown);
        }
        if let Some(member) = crate::standard_library::builtin_member_type(object_type, field) {
            return member;
        }
        if field == "into_iter"
            && let Type::Named { name, .. } = object_type
            && rils_builtins::builtin(name)
                .is_some_and(|definition| definition.member("next").is_some())
        {
            return Type::function(Vec::new(), object_type.clone());
        }
        if field == "clone"
            && crate::standard_library::builtin_owner_name(object_type)
                .is_some_and(|owner| rils_builtins::native_implements(owner, "Clone"))
            && let Some(member) =
                crate::standard_library::builtin_trait_member_type("Clone", object_type, field)
        {
            return member;
        }
        if let Type::Named { name, arguments } = object_type
            && arguments.is_empty()
            && let Some(signature) = self.host_functions.get(&format!("{name}::{field}"))
        {
            return match signature.parameters.as_ref() {
                Some(parameters) => FunctionSignature::fixed(
                    parameters.iter().skip(1).cloned().collect(),
                    signature.return_type.clone(),
                ),
                None => FunctionSignature::variadic(signature.return_type.clone()),
            }
            .as_type();
        }
        // Manifest functions use module paths for their names (for example
        // `unity_engine::game_object::transform`) while the receiver type is
        // carried as the first signature parameter.
        if let Type::Named { name, arguments } = object_type
            && arguments.is_empty()
            && let Some(signature) =
                self.host_functions
                    .iter()
                    .find_map(|(qualified, signature)| {
                        let member = qualified.rsplit("::").next()?;
                        let parameters = signature.parameters.as_ref()?;
                        (member == field && parameters.first() == Some(&Type::named(name)))
                            .then_some(signature)
                    })
        {
            return match signature.parameters.as_ref() {
                Some(parameters) => FunctionSignature::fixed(
                    parameters.iter().skip(1).cloned().collect(),
                    signature.return_type.clone(),
                ),
                None => FunctionSignature::variadic(signature.return_type.clone()),
            }
            .as_type();
        }
        let Type::Named { name, arguments } = object_type else {
            return Type::Unknown;
        };
        let iterator_item = self.iterable_item_type(object_type);
        self.types.get(name).map_or(Type::Unknown, |definition| {
            let substitutions = definition
                .generic_parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned())
                .collect::<HashMap<_, _>>();
            let member = definition
                .fields
                .get(field)
                .or_else(|| definition.methods.get(field))
                .cloned()
                .or_else(|| {
                    ((definition.implemented_traits.contains("Iterator") || name == "Iter")
                        && rils_builtins::is_iterator_default_method(field))
                    .then(|| {
                        crate::standard_library::builtin_trait_member_type_with_iterator_item(
                            "Iterator",
                            object_type,
                            field,
                            Some(iterator_item),
                        )
                    })
                    .flatten()
                });
            member
                .map(|member| member.substitute(&substitutions))
                .unwrap_or(Type::Unknown)
        })
    }

    fn iterable_item_type(&self, iterable_type: &Type) -> Type {
        self.iterable_item_type_inner(iterable_type, 0)
    }

    fn iterable_item_type_inner(&self, iterable_type: &Type, depth: usize) -> Type {
        if depth >= 8 {
            return Type::Unknown;
        }
        match iterable_type {
            Type::Reference { inner, .. } => self.iterable_item_type_inner(inner, depth + 1),
            Type::Array { element, .. } => (**element).clone(),
            Type::Named { name, arguments } => match name.as_str() {
                "Vec" | "VecDeque" | "BinaryHeap" | "HashSet" | "BTreeSet" | "OwnedIterator"
                | "Iter" | "Range" => arguments.first().cloned().unwrap_or(Type::Unknown),
                "HashMap" | "BTreeMap" if arguments.len() == 2 => Type::Tuple(arguments.clone()),
                _ => {
                    let Some(definition) = self.types.get(name) else {
                        return Type::Unknown;
                    };
                    let substitutions = definition
                        .generic_parameters
                        .iter()
                        .cloned()
                        .zip(arguments.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    if let Some(item) = definition
                        .associated_types
                        .get(&(String::from("IntoIterator"), String::from("Item")))
                    {
                        return item.substitute(&substitutions);
                    }
                    if let Some(item) = definition
                        .associated_types
                        .get(&("Iterator".into(), "Item".into()))
                    {
                        return item.substitute(&substitutions);
                    }
                    let Some(iterator) = definition
                        .associated_types
                        .get(&("IntoIterator".into(), "IntoIter".into()))
                    else {
                        return Type::Unknown;
                    };
                    self.iterable_item_type_inner(&iterator.substitute(&substitutions), depth + 1)
                }
            },
            _ => Type::Unknown,
        }
    }

    fn define_binding(&mut self, name: &str, span: Span, binding: Binding) {
        self.result.binding_types.insert(span, binding.ty.clone());
        self.scopes
            .last_mut()
            .expect("scope exists")
            .insert(name.into(), binding);
    }

    fn type_hint(&mut self, span: Span, ty: Type, prefix: &'static str) {
        if is_known(&ty) {
            self.result.hints.push(RawTypeHint {
                position: span.end,
                span,
                ty,
                prefix,
            });
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn with_scope_value<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(HashMap::new());
        let result = action(self);
        self.scopes.pop();
        result
    }
}

fn snake_case(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    for (index, character) in name.chars().enumerate() {
        if character.is_uppercase() {
            if index != 0 {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn resolve_impl_self(ty: &Type, target: &Type) -> Type {
    match ty {
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => target.clone(),
        Type::Option(inner) => Type::Option(Box::new(resolve_impl_self(inner, target))),
        Type::Result(ok, error) => Type::Result(
            Box::new(resolve_impl_self(ok, target)),
            Box::new(resolve_impl_self(error, target)),
        ),
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| resolve_impl_self(element, target))
                .collect(),
        ),
        Type::Array { element, length } => Type::Array {
            element: Box::new(resolve_impl_self(element, target)),
            length: *length,
        },
        Type::Reference { mutable, inner } => Type::Reference {
            mutable: *mutable,
            inner: Box::new(resolve_impl_self(inner, target)),
        },
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters.as_ref().map(|parameters| {
                parameters
                    .iter()
                    .map(|parameter| resolve_impl_self(parameter, target))
                    .collect()
            }),
            return_type: Box::new(resolve_impl_self(return_type, target)),
        },
        Type::Named { name, arguments } => Type::Named {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| resolve_impl_self(argument, target))
                .collect(),
        },
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(resolve_impl_self(base, target)),
            trait_name: trait_name.clone(),
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| resolve_impl_self(argument, target))
                .collect(),
        },
        other => other.clone(),
    }
}

fn literal_type(literal: &Literal, id: ExprId) -> Type {
    match literal {
        Literal::Unit => Type::Unit,
        Literal::Bool(_) => Type::Bool,
        Literal::I8(_) => Type::Integer(crate::types::IntegerType::I8),
        Literal::I16(_) => Type::Integer(crate::types::IntegerType::I16),
        Literal::I32(_) => Type::I32,
        Literal::I64(_) => Type::Integer(crate::types::IntegerType::I64),
        Literal::I128(_) => Type::Integer(crate::types::IntegerType::I128),
        Literal::Isize(_) => Type::Integer(crate::types::IntegerType::Isize),
        Literal::U8(_) => Type::Integer(crate::types::IntegerType::U8),
        Literal::U16(_) => Type::Integer(crate::types::IntegerType::U16),
        Literal::U32(_) => Type::Integer(crate::types::IntegerType::U32),
        Literal::U64(_) => Type::Integer(crate::types::IntegerType::U64),
        Literal::U128(_) => Type::Integer(crate::types::IntegerType::U128),
        Literal::Usize(_) => Type::USIZE,
        Literal::F32(_) => Type::Float(crate::types::FloatType::F32),
        Literal::F64(_) => Type::F64,
        Literal::Char(_) => Type::Char,
        Literal::Integer(_) => Type::IntegerInference(id),
        Literal::Float(_) => Type::FloatInference(id),
        Literal::String(_) => Type::String,
    }
}

fn option_inner(ty: Option<Type>) -> Type {
    match ty {
        Some(Type::Option(inner)) => *inner,
        _ => Type::Unknown,
    }
}

fn value_inner(ty: Option<Type>) -> Type {
    match ty {
        Some(Type::Option(inner)) | Some(Type::Result(inner, _)) => *inner,
        _ => Type::Unknown,
    }
}

fn merge_all(types: impl IntoIterator<Item = Type>) -> Type {
    types
        .into_iter()
        .reduce(|left, right| merge_types(&left, &right).unwrap_or(Type::Unknown))
        .unwrap_or(Type::Unit)
}

fn inferred_return(explicit_returns: Vec<Type>, tail: Type) -> Type {
    if explicit_returns.is_empty() {
        tail
    } else if tail == Type::Unit {
        merge_all(explicit_returns)
    } else {
        merge_all(explicit_returns.into_iter().chain([tail]))
    }
}

fn function_call_result(function: &Type, arguments: &[Type]) -> Type {
    let Type::Function {
        parameters,
        return_type,
    } = function
    else {
        return Type::Unknown;
    };
    let mut substitutions = HashMap::new();
    if let Some(parameters) = parameters {
        if parameters.len() != arguments.len() {
            return Type::Unknown;
        }
        for (expected, actual) in parameters.iter().zip(arguments) {
            infer_type_variables(expected, actual, &mut substitutions);
        }
    }
    return_type.substitute(&substitutions)
}

fn infer_type_variables(expected: &Type, actual: &Type, substitutions: &mut HashMap<String, Type>) {
    // Static checking reports conflicts; inference keeps the last consistent bindings.
    let _ = crate::types::infer_generic_arguments(expected, actual, substitutions);
}

fn is_known(ty: &Type) -> bool {
    match ty {
        Type::Unknown => false,
        Type::Option(inner) => is_known(inner),
        Type::Result(ok, error) => is_known(ok) && is_known(error),
        Type::Reference { inner, .. } => is_known(inner),
        Type::Function {
            parameters: Some(parameters),
            return_type,
        } => parameters.iter().all(is_known) && is_known(return_type),
        Type::Function {
            parameters: None, ..
        } => false,
        Type::Named { arguments, .. } => arguments.iter().all(is_known),
        _ => true,
    }
}
