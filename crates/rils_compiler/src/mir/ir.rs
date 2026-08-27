use crate::{
    ast::{BinaryOp, UnaryOp},
    hir::{
        FunctionId, HirIteratorMethods, HirLiteral, HirPattern, HirTraitImplementation,
        HirTypeDefinition, LocalId, TypeId,
    },
    source::{SourceFile, Span},
    types::IntegerType,
};
use rils_builtins::BuiltinId;

pub type BlockId = usize;
pub type Register = usize;
pub type ConstantId = usize;

pub struct MirPlace {
    pub local: LocalId,
    pub projections: Vec<MirProjection>,
}

pub enum MirProjection {
    Field(String),
    Index(Register),
}

pub struct MirProgram {
    pub sources: Vec<SourceFile>,
    pub functions: Vec<MirFunction>,
    pub types: Vec<HirTypeDefinition>,
    pub iterators: std::collections::HashMap<String, HirIteratorMethods>,
    pub trait_implementations: Vec<HirTraitImplementation>,
    pub entry: FunctionId,
}

pub struct MirFunction {
    pub name: String,
    pub exported: bool,
    pub blocks: Vec<BasicBlock>,
    pub constants: Vec<HirLiteral>,
    pub register_count: usize,
    pub local_count: usize,
    pub local_mutability: Vec<bool>,
    pub parameter_count: usize,
    pub capture_count: usize,
    pub span: Span,
}

pub struct BasicBlock {
    pub instructions: Vec<SpannedInstruction>,
    pub terminator: Option<SpannedTerminator>,
}

pub struct SpannedInstruction {
    pub instruction: MirInstruction,
    pub span: Span,
}

pub enum MirInstruction {
    LoadConstant {
        destination: Register,
        constant: ConstantId,
    },
    LoadFunction {
        destination: Register,
        function: FunctionId,
    },
    BindMethod {
        destination: Register,
        function: FunctionId,
        receiver: Register,
    },
    BorrowTemporary {
        destination: Register,
        source: Register,
        mutable: bool,
    },
    Reborrow {
        destination: Register,
        source: Register,
        mutable: bool,
    },
    CreateClosure {
        destination: Register,
        function: FunctionId,
        captures: Vec<LocalId>,
    },
    TakeLocal {
        destination: Register,
        local: LocalId,
    },
    TakePlace {
        destination: Register,
        place: MirPlace,
    },
    StoreLocal {
        local: LocalId,
        source: Register,
    },
    InitLocal {
        local: LocalId,
        source: Register,
    },
    DropLocal {
        local: LocalId,
    },
    BorrowLocal {
        destination: Register,
        local: LocalId,
        mutable: bool,
    },
    BorrowPlace {
        destination: Register,
        place: MirPlace,
        mutable: bool,
    },
    Dereference {
        destination: Register,
        source: Register,
    },
    StoreDereference {
        reference: Register,
        source: Register,
    },
    StorePlace {
        place: MirPlace,
        source: Register,
    },
    IntoIterator {
        destination: Register,
        source: Register,
    },
    Move {
        destination: Register,
        source: Register,
    },
    Unary {
        destination: Register,
        operator: UnaryOp,
        operand: Register,
    },
    Cast {
        destination: Register,
        source: Register,
        target: IntegerType,
    },
    Binary {
        destination: Register,
        left: Register,
        operator: BinaryOp,
        right: Register,
        integer: Option<IntegerType>,
    },
    Call {
        destination: Register,
        function: FunctionId,
        arguments: Vec<Register>,
    },
    CallValue {
        destination: Register,
        callee: Register,
        arguments: Vec<Register>,
    },
    CallImport {
        destination: Register,
        name: String,
        signature: crate::types::FunctionSignature,
        capability: String,
        arguments: Vec<Register>,
    },
    CallRuntime {
        destination: Register,
        builtin: BuiltinId,
        arguments: Vec<Register>,
    },
    CallIntrinsic {
        destination: Register,
        intrinsic: BuiltinId,
        target: Option<IntegerType>,
        arguments: Vec<Register>,
    },
    ConstructRecord {
        destination: Register,
        type_id: TypeId,
        variant: Option<String>,
        fields: Vec<(String, Register)>,
    },
    ConstructTupleVariant {
        destination: Register,
        type_id: TypeId,
        variant: String,
        fields: Vec<Register>,
    },
    ConstructUnitVariant {
        destination: Register,
        type_id: TypeId,
        variant: String,
    },
    BuildTuple {
        destination: Register,
        elements: Vec<Register>,
    },
    BuildArray {
        destination: Register,
        elements: Vec<Register>,
    },
    BuildRepeatArray {
        destination: Register,
        value: Register,
        count: Register,
    },
    BuildRange {
        destination: Register,
        start: Register,
        end: Register,
    },
    BuildOptionNone {
        destination: Register,
    },
    BuildOptionSome {
        destination: Register,
        source: Register,
    },
    BuildResultOk {
        destination: Register,
        source: Register,
    },
    BuildResultErr {
        destination: Register,
        source: Register,
    },
    TryResult {
        destination: Register,
        source: Register,
    },
    MatchPattern {
        destination: Register,
        source: Register,
        pattern: HirPattern,
    },
    BindPattern {
        source: Register,
        pattern: HirPattern,
    },
}

pub struct SpannedTerminator {
    pub terminator: MirTerminator,
    pub span: Span,
}

pub enum MirTerminator {
    Goto(BlockId),
    Branch {
        condition: Register,
        then_block: BlockId,
        else_block: BlockId,
    },
    IteratorNext {
        iterator: Register,
        destination: Register,
        some_block: BlockId,
        none_block: BlockId,
    },
    MatchFail,
    Return(Register),
}
