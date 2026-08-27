use std::collections::HashMap;

use crate::{
    ast::{BinaryOp, EnumVariant, GenericParameter, LogicalOp, NamedField, UnaryOp},
    source::{SourceFile, Span},
    types::{FunctionSignature, IntegerType},
};
use rils_builtins::BuiltinId;

pub type LocalId = usize;
pub type FunctionId = usize;
pub type TypeId = usize;

pub struct HirPlace {
    pub local: LocalId,
    pub projections: Vec<HirProjection>,
}

pub enum HirProjection {
    Field(String),
    Index(Box<HirExpression>),
}

pub struct HirProgram {
    pub sources: Vec<SourceFile>,
    pub functions: Vec<HirFunction>,
    pub types: Vec<HirTypeDefinition>,
    pub iterators: HashMap<String, HirIteratorMethods>,
    pub trait_implementations: Vec<HirTraitImplementation>,
    pub entry: FunctionId,
}

#[derive(Clone)]
pub struct HirTraitImplementation {
    pub target: String,
    pub trait_name: String,
    pub source: crate::source::SourceId,
    pub methods: HashMap<String, FunctionId>,
}

#[derive(Clone, Default)]
pub struct HirIteratorMethods {
    pub into_iter: Option<FunctionId>,
    pub next: Option<FunctionId>,
}

pub enum HirTypeDefinition {
    Struct {
        name: String,
        generic_parameters: Vec<GenericParameter>,
        fields: Vec<NamedField>,
    },
    Enum {
        name: String,
        generic_parameters: Vec<GenericParameter>,
        variants: Vec<EnumVariant>,
    },
}

pub struct HirFunction {
    pub name: String,
    pub exported: bool,
    pub parameter_count: usize,
    pub capture_count: usize,
    pub local_count: usize,
    pub local_mutability: Vec<bool>,
    pub statements: Vec<HirStatement>,
    pub span: Span,
}

pub enum HirStatement {
    DefineFunction {
        local: LocalId,
        function: FunctionId,
        captures: Vec<LocalId>,
        span: Span,
    },
    Let {
        local: LocalId,
        initializer: HirExpression,
        span: Span,
    },
    While {
        condition: HirExpression,
        body: Vec<HirStatement>,
        span: Span,
    },
    Loop {
        body: Vec<HirStatement>,
        span: Span,
    },
    For {
        binding: LocalId,
        iterable: HirExpression,
        body: Vec<HirStatement>,
        span: Span,
    },
    Return {
        value: Option<HirExpression>,
        span: Span,
    },
    Break {
        value: Option<HirExpression>,
        span: Span,
    },
    Continue {
        span: Span,
    },
    DropLocal {
        local: LocalId,
        span: Span,
    },
    Expression {
        expression: HirExpression,
        terminated: bool,
        span: Span,
    },
}

pub enum HirExpression {
    Literal {
        value: HirLiteral,
        span: Span,
    },
    Local {
        local: LocalId,
        span: Span,
    },
    Function {
        function: FunctionId,
        span: Span,
    },
    BindMethod {
        function: FunctionId,
        receiver: Box<HirExpression>,
        span: Span,
    },
    BorrowTemporary {
        value: Box<HirExpression>,
        mutable: bool,
        span: Span,
    },
    Reborrow {
        reference: Box<HirExpression>,
        mutable: bool,
        span: Span,
    },
    Place {
        place: HirPlace,
        span: Span,
    },
    Assign {
        local: LocalId,
        value: Box<HirExpression>,
        span: Span,
    },
    AssignPlace {
        place: HirPlace,
        value: Box<HirExpression>,
        span: Span,
    },
    AssignDereference {
        reference: Box<HirExpression>,
        value: Box<HirExpression>,
        span: Span,
    },
    BorrowLocal {
        local: LocalId,
        mutable: bool,
        span: Span,
    },
    BorrowPlace {
        place: HirPlace,
        mutable: bool,
        span: Span,
    },
    Unary {
        operator: UnaryOp,
        operand: Box<HirExpression>,
        span: Span,
    },
    Cast {
        operand: Box<HirExpression>,
        target: IntegerType,
        span: Span,
    },
    Binary {
        left: Box<HirExpression>,
        operator: BinaryOp,
        right: Box<HirExpression>,
        integer: Option<IntegerType>,
        span: Span,
    },
    Logical {
        left: Box<HirExpression>,
        operator: LogicalOp,
        right: Box<HirExpression>,
        span: Span,
    },
    Call {
        function: FunctionId,
        arguments: Vec<HirExpression>,
        span: Span,
    },
    CallValue {
        callee: Box<HirExpression>,
        arguments: Vec<HirExpression>,
        span: Span,
    },
    CallImport {
        name: String,
        signature: FunctionSignature,
        capability: String,
        arguments: Vec<HirExpression>,
        span: Span,
    },
    CallIntrinsic {
        intrinsic: BuiltinId,
        target: Option<IntegerType>,
        arguments: Vec<HirExpression>,
        span: Span,
    },
    IntoIterator {
        value: Box<HirExpression>,
        span: Span,
    },
    ConstructRecord {
        type_id: TypeId,
        variant: Option<String>,
        fields: Vec<(String, HirExpression)>,
        span: Span,
    },
    ConstructTupleVariant {
        type_id: TypeId,
        variant: String,
        fields: Vec<HirExpression>,
        span: Span,
    },
    ConstructUnitVariant {
        type_id: TypeId,
        variant: String,
        span: Span,
    },
    Tuple {
        elements: Vec<HirExpression>,
        span: Span,
    },
    Array {
        elements: Vec<HirExpression>,
        repeat: Option<Box<HirExpression>>,
        span: Span,
    },
    Range {
        start: Box<HirExpression>,
        end: Box<HirExpression>,
        span: Span,
    },
    OptionNone {
        span: Span,
    },
    OptionSome {
        value: Box<HirExpression>,
        span: Span,
    },
    ResultOk {
        value: Box<HirExpression>,
        span: Span,
    },
    ResultErr {
        value: Box<HirExpression>,
        span: Span,
    },
    Try {
        operand: Box<HirExpression>,
        span: Span,
    },
    Match {
        value: Box<HirExpression>,
        arms: Vec<HirMatchArm>,
        span: Span,
    },
    If {
        condition: Box<HirExpression>,
        then_branch: Vec<HirStatement>,
        else_branch: Option<Box<HirExpression>>,
        span: Span,
    },
    Block {
        statements: Vec<HirStatement>,
        span: Span,
    },
}

pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub expression: HirExpression,
    pub span: Span,
}

#[derive(Clone)]
pub enum HirPattern {
    Wildcard,
    Binding(LocalId),
    Literal(HirLiteral),
    Some(Box<HirPattern>),
    None,
    Ok(Box<HirPattern>),
    Err(Box<HirPattern>),
    TupleVariant {
        path: Vec<String>,
        fields: Vec<HirPattern>,
    },
    Record {
        path: Vec<String>,
        fields: Vec<(String, HirPattern)>,
    },
    Path(Vec<String>),
}

#[derive(Clone)]
pub enum HirLiteral {
    Unit,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    F32(f32),
    F64(f64),
    Char(char),
    String(String),
}
