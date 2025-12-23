//! Version agnostic intermediate representation of CPython marshallable objects
//! and bytecode
//!
//! This module is intended to abstract over format differences for further
//! analysis, and in particular is intended to be abstractly interpreted

pub mod parse;

#[derive(Debug, Clone)]
pub enum Instruction {
    LoadConst(Constant),
    Load { from: UnresolvedPlace },
    Store { into: UnresolvedPlace },
    Pop,
    Copy(u32),
    Swap(u32),
    UnaryOp(UnaryOp),
    // Binary OP + Compare OP
    BinaryOp(BinOp),
    // Target is an absolute jump target
    Jump { class: JumpClass, target: u32 },
    Call(u32),
    Return,
    MakeFunction,
    // Implicit conversions
    Coercion(Coercion),
}

#[derive(Debug, Clone, Copy)]
pub enum UnresolvedPlace {
    Global(u32),
    Local(u32),
    Cell(u32),
    Name(u32),
}

#[derive(Debug, Clone, Copy)]
pub enum Constant {
    ByIndex(u32),
    SmallInt(u8),
    None,
    Null,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negative,
    LogicalNot,
    Invert,
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Power,
    Div,
    FloorDiv,
    Remainder,
    And,
    Or,
    Xor,
    LShift,
    RShift,
    MatMul,
    InplaceAdd,
    InplaceSub,
    InplaceMul,
    InplacePower,
    InplaceDiv,
    InplaceFloorDiv,
    InplaceRemainder,
    InplaceAnd,
    InplaceOr,
    InplaceXor,
    InplaceLShift,
    InplaceRShift,
    InplaceMatMul,
    Subscript,
    Eq,
    Ne,
    Gt,
    Lt,
    GtEq,
    LtEq,
    Is,
}

#[derive(Debug, Clone, Copy)]
pub enum JumpClass {
    Always,
    IfFalse,
}

#[derive(Debug, Clone, Copy)]
pub enum Coercion {
    Bool,
    Iter,
    Awaitable,
    AsyncIter,
}
