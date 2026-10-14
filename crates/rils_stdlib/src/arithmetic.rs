//! Primitive arithmetic contract implemented from standard-library family declarations.
use rils_syntax::ast::BinaryOp;

pub enum ArithmeticResult<T> {
    Number(T),
    Bool(bool),
}

pub trait PrimitiveArithmetic: Sized {
    fn binary(self, operator: BinaryOp, right: Self) -> Result<ArithmeticResult<Self>, String>;
    fn negate(self) -> Result<Self, String>;
}
