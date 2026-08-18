use crate::source::Span;
use crate::types::Type;

#[derive(Clone, Debug)]
pub struct Program {
    pub statements: Vec<Stmt>,
    pub type_references: Vec<TypeReference>,
    pub macros: Vec<MacroSymbol>,
}

#[derive(Clone, Debug)]
pub struct MacroSymbol {
    pub name: String,
    pub name_span: Span,
    pub references: Vec<Span>,
}

#[derive(Clone, Debug)]
pub struct TypeReference {
    pub name: String,
    pub span: Span,
    pub definition_span: Option<Span>,
    pub is_builtin: bool,
    pub arguments: Vec<Type>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub mutable: bool,
    pub type_annotation: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct GenericParameter {
    pub name: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct NamedField {
    pub name: String,
    pub type_annotation: Type,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub path: Vec<String>,
    pub arguments: Vec<Vec<String>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum EnumVariant {
    Unit {
        name: String,
        span: Span,
    },
    Tuple {
        name: String,
        fields: Vec<Type>,
        span: Span,
    },
    Record {
        name: String,
        fields: Vec<NamedField>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct ImplMethod {
    pub name: String,
    pub name_span: Span,
    pub generic_parameters: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub name: String,
    pub name_span: Span,
    pub generic_parameters: Vec<GenericParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct AssociatedType {
    pub name: String,
    pub name_span: Span,
    pub generic_parameters: Vec<GenericParameter>,
    pub value: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseImportKind {
    Single,
    Glob,
}

#[derive(Clone, Debug)]
pub struct UseImport {
    pub path: Vec<String>,
    pub path_spans: Vec<Span>,
    pub alias: Option<String>,
    pub alias_span: Option<Span>,
    pub name_span: Span,
    pub kind: UseImportKind,
    pub span: Span,
}

impl UseImport {
    pub fn binding_name(&self) -> Option<&str> {
        match self.kind {
            UseImportKind::Single => self
                .alias
                .as_deref()
                .or_else(|| self.path.last().map(String::as_str)),
            UseImportKind::Glob => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Public {
        statement: Box<Stmt>,
        span: Span,
    },
    Module {
        name: String,
        name_span: Span,
        statements: Option<Vec<Stmt>>,
        span: Span,
    },
    Use {
        imports: Vec<UseImport>,
        span: Span,
    },
    Let {
        name: String,
        name_span: Span,
        mutable: bool,
        type_annotation: Option<Type>,
        initializer: Expr,
        span: Span,
    },
    Function {
        name: String,
        name_span: Span,
        generic_parameters: Vec<GenericParameter>,
        parameters: Vec<Parameter>,
        return_type: Option<Type>,
        body: Block,
        span: Span,
    },
    Struct {
        attributes: Vec<Attribute>,
        name: String,
        name_span: Span,
        generic_parameters: Vec<GenericParameter>,
        fields: Vec<NamedField>,
        span: Span,
    },
    Enum {
        name: String,
        name_span: Span,
        generic_parameters: Vec<GenericParameter>,
        variants: Vec<EnumVariant>,
        span: Span,
    },
    TypeAlias {
        name: String,
        name_span: Span,
        generic_parameters: Vec<GenericParameter>,
        target: Type,
        span: Span,
    },
    Impl {
        generic_parameters: Vec<GenericParameter>,
        trait_name: Option<String>,
        target: Type,
        associated_types: Vec<AssociatedType>,
        methods: Vec<ImplMethod>,
        span: Span,
    },
    Trait {
        name: String,
        name_span: Span,
        bounds: Vec<String>,
        associated_types: Vec<AssociatedType>,
        methods: Vec<TraitMethod>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Block,
        span: Span,
    },
    Loop {
        body: Block,
        span: Span,
    },
    For {
        binding: String,
        binding_span: Span,
        iterable: Expr,
        body: Block,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        value: Option<Expr>,
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expr {
        expression: Expr,
        terminated: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    Dereference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Literal {
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
    Integer(i128),
    Float(f64),
    String(String),
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wildcard {
        span: Span,
    },
    Binding {
        name: String,
        span: Span,
    },
    Literal {
        value: Literal,
        span: Span,
    },
    Some {
        inner: Box<Pattern>,
        span: Span,
    },
    None {
        span: Span,
    },
    Ok {
        inner: Box<Pattern>,
        span: Span,
    },
    Err {
        inner: Box<Pattern>,
        span: Span,
    },
    TupleVariant {
        path: Vec<String>,
        fields: Vec<Pattern>,
        span: Span,
    },
    Record {
        path: Vec<String>,
        fields: Vec<(String, Pattern)>,
        span: Span,
    },
    Path {
        path: Vec<String>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard { span }
            | Self::Binding { span, .. }
            | Self::Literal { span, .. }
            | Self::Some { span, .. }
            | Self::None { span }
            | Self::Ok { span, .. }
            | Self::Err { span, .. }
            | Self::TupleVariant { span, .. }
            | Self::Record { span, .. }
            | Self::Path { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub expression: Expr,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Literal {
        value: Literal,
        span: Span,
    },
    Variable {
        name: String,
        span: Span,
    },
    Path {
        segments: Vec<String>,
        span: Span,
    },
    QualifiedPath {
        target: Type,
        trait_name: String,
        member: String,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        name: String,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Tuple {
        elements: Vec<Expr>,
        span: Span,
    },
    Array {
        elements: Vec<Expr>,
        repeat: Option<Box<Expr>>,
        span: Span,
    },
    Try {
        operand: Box<Expr>,
        span: Span,
    },
    RecordLiteral {
        path: Vec<String>,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    Borrow {
        mutable: bool,
        target: Box<Expr>,
        span: Span,
    },
    Unary {
        operator: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Cast {
        operand: Box<Expr>,
        target: Type,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        operator: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Logical {
        left: Box<Expr>,
        operator: LogicalOp,
        right: Box<Expr>,
        span: Span,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
        span: Span,
    },
    If {
        condition: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Block(Block),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Literal { span, .. }
            | Self::Variable { span, .. }
            | Self::Path { span, .. }
            | Self::QualifiedPath { span, .. }
            | Self::Member { span, .. }
            | Self::Index { span, .. }
            | Self::Tuple { span, .. }
            | Self::Array { span, .. }
            | Self::Try { span, .. }
            | Self::RecordLiteral { span, .. }
            | Self::Assign { span, .. }
            | Self::Borrow { span, .. }
            | Self::Unary { span, .. }
            | Self::Cast { span, .. }
            | Self::Binary { span, .. }
            | Self::Logical { span, .. }
            | Self::Range { span, .. }
            | Self::Call { span, .. }
            | Self::If { span, .. }
            | Self::Match { span, .. } => *span,
            Self::Block(block) => block.span,
        }
    }
}
