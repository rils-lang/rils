//! VM instruction operands; disk encoding lives in format/instruction.

use super::*;

#[derive(Clone)]
pub(super) enum Instruction {
    ApplyStorage {
        destination: usize,
        source: usize,
        expected: Type,
    },
    LoadConstant {
        destination: usize,
        constant: usize,
    },
    LoadFunction {
        destination: usize,
        function: usize,
    },
    BindMethod {
        destination: usize,
        function: usize,
        receiver: usize,
    },
    BorrowTemporary {
        destination: usize,
        source: usize,
        mutable: bool,
    },
    Reborrow {
        destination: usize,
        source: usize,
        mutable: bool,
    },
    CreateClosure {
        destination: usize,
        function: usize,
        captures: Vec<usize>,
    },
    TakeLocal {
        destination: usize,
        local: usize,
    },
    TakePlace {
        destination: usize,
        place: BytecodePlace,
    },
    StoreLocal {
        local: usize,
        source: usize,
    },
    InitLocal {
        local: usize,
        source: usize,
        type_annotation: Option<Type>,
    },
    DropLocal {
        local: usize,
    },
    BorrowLocal {
        destination: usize,
        local: usize,
        mutable: bool,
    },
    BorrowPlace {
        destination: usize,
        place: BytecodePlace,
        mutable: bool,
    },
    Dereference {
        destination: usize,
        source: usize,
    },
    StoreDereference {
        reference: usize,
        source: usize,
    },
    StorePlace {
        place: BytecodePlace,
        source: usize,
    },
    IntoIterator {
        destination: usize,
        source: usize,
    },
    Move {
        destination: usize,
        source: usize,
    },
    Unary {
        destination: usize,
        operator: UnaryOp,
        operand: usize,
    },
    Cast {
        destination: usize,
        source: usize,
        target: IntegerType,
    },
    Binary {
        destination: usize,
        left: usize,
        operator: BinaryOp,
        right: usize,
    },
    IntegerBinary {
        destination: usize,
        left: usize,
        operator: BinaryOp,
        right: usize,
        integer: IntegerType,
    },
    Call {
        destination: usize,
        function: usize,
        arguments: Vec<usize>,
    },
    CallValue {
        destination: usize,
        callee: usize,
        arguments: Vec<usize>,
    },
    CallImport {
        destination: usize,
        import: usize,
        arguments: Vec<usize>,
    },
    CallNative {
        destination: usize,
        import: usize,
        arguments: Vec<usize>,
    },
    ConstructRecord {
        destination: usize,
        type_id: usize,
        expected: Type,
        variant: Option<String>,
        fields: Vec<(String, usize)>,
    },
    ConstructTupleVariant {
        destination: usize,
        type_id: usize,
        expected: Type,
        variant: String,
        fields: Vec<usize>,
    },
    ConstructUnitVariant {
        destination: usize,
        type_id: usize,
        expected: Type,
        variant: String,
    },
    BuildTuple {
        destination: usize,
        elements: Vec<usize>,
    },
    BuildArray {
        destination: usize,
        elements: Vec<usize>,
    },
    BuildRepeatArray {
        destination: usize,
        value: usize,
        count: usize,
    },
    BuildRange {
        destination: usize,
        start: usize,
        end: usize,
    },
    BuildOptionNone {
        destination: usize,
        item_type: Option<Type>,
    },
    BuildOptionSome {
        destination: usize,
        source: usize,
    },
    BuildResultOk {
        destination: usize,
        source: usize,
    },
    BuildResultErr {
        destination: usize,
        source: usize,
    },
    TryResult {
        destination: usize,
        source: usize,
    },
    MatchPattern {
        destination: usize,
        source: usize,
        pattern: HirPattern,
    },
    BindPattern {
        source: usize,
        pattern: HirPattern,
    },
    Jump {
        target: usize,
    },
    Branch {
        condition: usize,
        then_target: usize,
        else_target: usize,
    },
    IteratorNext {
        iterator: usize,
        destination: usize,
        some_target: usize,
        none_target: usize,
    },
    Return {
        source: usize,
    },
    MatchFail,
}
