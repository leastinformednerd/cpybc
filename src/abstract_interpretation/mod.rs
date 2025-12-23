//! Effectively this is a higher level abstraction over [`stack_ir`] that turns
//! stack based instructions into a set of blocks. Each block is a list of
//! statements which optionally take an expression
//!
//! Expressions are recursively defined as being the results of loads, either
//! of constants or of variables. No tracking is done here about data flow
//! between uses of a variable

// TODO: Move these out to a common core
use crate::stack_ir::{BinOp, Coercion, Constant, UnaryOp, UnresolvedPlace};

pub mod eval;

// I need to figure out a nice way to handle this that doesn't require so much
// cloning. Some sort of interning I guess
#[derive(Debug, Clone)]
pub enum Expr {
    // Primitive values
    Constant(Constant),
    Load {
        from: Place,
    },

    // Operation results
    UnaryOp(UnaryOp, Box<Expr>),
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Coercion(Coercion, Box<Expr>),
    MakeFunction(Box<Expr>),

    // Function calls
    Call {
        func: Box<Expr>,
        receiver: Box<Expr>,
        args: Box<[Expr]>,
    },
}

#[derive(Debug)]
pub enum Statement {
    Trivial(Expr),
    Store { expr: Expr, into: Place },
    Return(Expr),
    If { expr: Expr, target: u32 },
    Jump { target: u32 },
}

#[derive(Debug)]
pub struct Block {
    pub body: Box<[Statement]>,
    pub control_flow: ControlFlow,
}

#[derive(Debug)]
pub enum ControlFlow {
    // The end of the block is an unconditional jump or "falls through" to the
    // the next block
    Unconditional(u32),
    // The end of the block contains a conditional jump
    CondtionalJump {
        if_true: u32,
        if_false: u32,
        expr: Expr,
    },
    // This block either returns or contains the final instruction
    Terminates,
}

#[derive(Debug, Clone, Copy)]
pub enum Place {
    Local(u32),
    Global(u32),
    Cell(u32),
}

impl Place {
    pub fn from_unresolved_unchecked(from: &UnresolvedPlace) -> Place {
        match from {
            UnresolvedPlace::Local(n) => Place::Local(*n),
            UnresolvedPlace::Global(n) => Place::Global(*n),
            UnresolvedPlace::Cell(n) => Place::Cell(*n),
            UnresolvedPlace::Name(_) => unreachable!(),
        }
    }
}
