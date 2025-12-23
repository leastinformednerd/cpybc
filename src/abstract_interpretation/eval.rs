//! Do abstract interpretation on the [`stack_ir`] to turn it into a simple
//! block structure. This makes an assumption that the program does not place
//! things on the stack except for in the context of a statement, i.e. that the
//! stack is always empty at the start and end of every block

use crate::{
    abstract_interpretation::ControlFlow,
    objects::{CodeObject, PyObjectRegion},
    stack_ir::{self, Instruction, JumpClass},
};

use super::{Block, Expr, Statement};
use std::{
    collections::{BTreeSet, HashMap},
    ops::Range,
};

pub(crate) struct EvalCtx<'a> {
    code: Box<[Instruction]>,
    code_obj: &'a CodeObject<'a>,
    pub(crate) region: &'a PyObjectRegion,

    stack: Vec<Expr>,
    out_blocks: HashMap<u32, Block>,
}

impl<'a> EvalCtx<'a> {
    fn new(
        code: Box<[Instruction]>,
        max_stack: usize,
        code_obj: &'a CodeObject<'a>,
        region: &'a PyObjectRegion,
    ) -> EvalCtx<'a> {
        EvalCtx {
            code,
            code_obj,
            region,
            stack: Vec::with_capacity(max_stack),
            out_blocks: HashMap::new(),
        }
    }

    fn go(&mut self) -> Result<(), EvaluationError> {
        for block in self.blocks() {
            println!(
                "{:#?}",
                &self.code[block.start as usize..block.end as usize]
            );
        }
        for block in self.blocks() {
            let start = block.start;
            let block = self.process_block(block)?;
            self.out_blocks.insert(start, block);
        }

        Ok(())
    }

    fn process_block(&mut self, bounds: Range<u32>) -> Result<Block, EvaluationError> {
        let code_bounds = bounds.start as usize..bounds.end as usize;
        // It is actually possible to take this as owned since each block is
        // guaranteed to not overlap, but it's not super important
        let code = &self.code[code_bounds];
        self.stack.clear();

        let mut statements = Vec::new();

        for instruction in code {
            match instruction {
                Instruction::LoadConst(constant) => self.stack.push(Expr::Constant(*constant)),
                Instruction::Load { from } => self.stack.push(Expr::Load {
                    from: self.code_obj.eval_place(from, self),
                }),
                Instruction::Store { into } => match self.stack.pop() {
                    Some(expr) => statements.push(Statement::Store {
                        expr,
                        into: self.code_obj.eval_place(into, self),
                    }),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::Pop => match self.stack.pop() {
                    Some(expr) => statements.push(Statement::Trivial(expr)),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::Copy(n) => match self.stack.get(self.stack.len() - 1 - *n as usize) {
                    Some(val) => self.stack.push(val.clone()),
                    None => return Err(EvaluationError::StackOpOutOfBounds),
                },
                Instruction::Swap(n) => {
                    if *n == 0 {
                        continue;
                    }
                    let len = self.stack.len();
                    match self
                        .stack
                        .get_disjoint_mut([len - 1 - (*n as usize), len - 1])
                    {
                        Ok([a, b]) => std::mem::swap(a, b),
                        _ => continue,
                    }
                }
                Instruction::BinaryOp(op) => match (self.stack.pop(), self.stack.pop()) {
                    (Some(rhs), Some(lhs)) => self.stack.push(Expr::BinaryOp {
                        op: *op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    }),
                    _ => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::UnaryOp(unary_op) => match self.stack.pop() {
                    Some(expr) => self.stack.push(Expr::UnaryOp(*unary_op, Box::new(expr))),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target,
                } => match self.stack.pop() {
                    Some(expr) => statements.push(Statement::If {
                        expr,
                        target: *target,
                    }),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::Jump {
                    class: JumpClass::Always,
                    target,
                } => statements.push(Statement::Jump { target: *target }),
                Instruction::Call(n) => {
                    let mut args = Vec::with_capacity(*n as usize);
                    for _ in 0..*n {
                        match self.stack.pop() {
                            Some(expr) => args.push(expr),
                            None => return Err(EvaluationError::PoppedEmptyStack),
                        }
                    }
                    let receiver = match self.stack.pop() {
                        Some(expr) => expr,
                        None => return Err(EvaluationError::PoppedEmptyStack),
                    };
                    let func = match self.stack.pop() {
                        Some(expr) => expr,
                        None => return Err(EvaluationError::PoppedEmptyStack),
                    };
                    self.stack.push(Expr::Call {
                        func: Box::new(func),
                        receiver: Box::new(receiver),
                        args: args.into_boxed_slice(),
                    });
                }
                Instruction::Return => match self.stack.pop() {
                    Some(expr) => statements.push(Statement::Return(expr)),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::MakeFunction => match self.stack.pop() {
                    Some(expr) => self.stack.push(Expr::MakeFunction(Box::new(expr))),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
                Instruction::Coercion(coercion) => match self.stack.pop() {
                    Some(expr) => self.stack.push(Expr::Coercion(*coercion, Box::new(expr))),
                    None => return Err(EvaluationError::PoppedEmptyStack),
                },
            }
        }

        if !self.stack.is_empty() {
            return Err(EvaluationError::BlockWithNonEmptyStack(
                self.stack.clone(),
                code.iter().map(Clone::clone).collect(),
            ));
        }

        let control_flow = match statements.last() {
            Some(Statement::Return(_)) => ControlFlow::Terminates,
            Some(Statement::If { expr: _, target: _ }) => {
                let Some(Statement::If {
                    expr,
                    target: if_false,
                }) = statements.pop()
                else {
                    unreachable!()
                };
                ControlFlow::CondtionalJump {
                    if_true: (&bounds).end as u32,
                    if_false: if_false,
                    expr,
                }
            }
            Some(Statement::Jump { target: _ }) => {
                let Some(Statement::Jump { target }) = statements.pop() else {
                    unreachable!()
                };
                ControlFlow::Unconditional(target)
            }
            _ if bounds.end as usize == self.code.len() => ControlFlow::Terminates,
            _ => ControlFlow::Unconditional(bounds.end),
        };

        Ok(Block {
            body: statements.into_boxed_slice(),
            control_flow,
        })
    }

    fn blocks(&self) -> Vec<Range<u32>> {
        let mut boundaries = BTreeSet::new();
        boundaries.insert(0);
        boundaries.insert(self.code.len() as u32);
        for (idx, instr) in self.code.iter().enumerate() {
            match instr {
                Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target,
                } => {
                    boundaries.insert(*target);
                    boundaries.insert((idx as u32) + 1);
                }
                Instruction::Jump {
                    class: JumpClass::Always,
                    target,
                } => {
                    boundaries.insert(*target);
                    boundaries.insert((idx as u32) + 1);
                }
                Instruction::Return => {
                    boundaries.insert((idx as u32) + 1);
                }
                _ => continue,
            }
        }

        let mut out = Vec::with_capacity(boundaries.len() + 1);
        boundaries.into_iter().reduce(|prev, next| {
            out.push(prev..next);
            next
        });
        out
    }
}

#[derive(Debug)]
pub enum EvaluationError {
    ParseError(stack_ir::parse::IRParseError),
    PoppedEmptyStack,
    StackOpOutOfBounds,
    BlockWithNonEmptyStack(Vec<Expr>, Vec<Instruction>),
}

impl From<stack_ir::parse::IRParseError> for EvaluationError {
    fn from(value: stack_ir::parse::IRParseError) -> Self {
        Self::ParseError(value)
    }
}

pub fn eval314(
    input: CodeObject,
    region: &PyObjectRegion,
) -> Result<HashMap<u32, Block>, EvaluationError> {
    let instrs = stack_ir::parse::parse314(input.code(&region))?;
    let mut ctx = EvalCtx::new(
        instrs.into_boxed_slice(),
        input.stack_size() as usize,
        &input,
        region,
    );
    ctx.go()?;

    Ok(ctx.out_blocks)
}
