use crate::stack_ir::{BinOp, Coercion, JumpClass, UnaryOp, UnresolvedPlace};

use super::{Constant, Instruction};

#[derive(Debug)]
pub enum IRParseError {
    SmallIntTooLarge(u32),
    OutOfBoundsBinOp(u32),
    OutOfBoundsCompareOp(u32),
    ArgExtendWouldOverflow(u32),
    NotYetImplementedInstruction(u8),
    JumpPastEnd(u32),
    JumpBeforeStart(u32),
}

pub fn parse314(code: &[u8]) -> Result<Vec<Instruction>, IRParseError> {
    let code = as_tuple(code);
    let mut out = Vec::new();
    let mut mapping = Vec::new();

    let mut arg_extension = 0u32;
    macro_rules! extend_arg {
        ($base:expr) => {{
            let _intermediate = ($base as u32) + arg_extension;
            arg_extension = 0;
            _intermediate
        }};
    }

    let mut instruction_count = 0;
    macro_rules! push {
        ($val:expr) => {{
            out.push($val);
            mapping.push(instruction_count);
        }};
    }

    for operation in code {
        // TODO: Remove all the magic numbers (and in general make this easier
        // to generalise to all python versions).
        match operation {
            // Load consts
            (82, idx) => {
                arg_extension = 0;
                push!(Instruction::LoadConst(Constant::ByIndex(extend_arg!(*idx))));
            }
            (94, n) => {
                let n2 = extend_arg!(*n);
                if n2 > 255 {
                    return Err(IRParseError::SmallIntTooLarge(n2));
                }
                push!(Instruction::LoadConst(Constant::SmallInt(*n)))
            }
            (33, _) => {
                arg_extension = 0;
                push!(Instruction::LoadConst(Constant::Null))
            }

            // Loads
            (92, arg) => {
                push!(Instruction::LoadConst(Constant::Null));
                push!(Instruction::Load {
                    from: UnresolvedPlace::Global(extend_arg!((*arg) >> 1)),
                });
            }
            (83 | 84 | 85 | 86 | 88, arg) => push!(Instruction::Load {
                from: UnresolvedPlace::Local(extend_arg!(*arg)),
            }),
            (87 | 89, arg) => {
                let arg = extend_arg!(*arg);
                push!(Instruction::Load {
                    from: UnresolvedPlace::Local(arg >> 4),
                });
                push!(Instruction::Load {
                    from: UnresolvedPlace::Local(arg & 15),
                });
            }
            (93, arg) => push!(Instruction::Load {
                from: UnresolvedPlace::Name(extend_arg!(*arg))
            }),

            // Stores
            (112, arg) => push!(Instruction::Store {
                into: UnresolvedPlace::Local(extend_arg!(*arg)),
            }),
            (115, arg) => push!(Instruction::Store {
                into: UnresolvedPlace::Global(extend_arg!(*arg)),
            }),
            (114, arg) => {
                let arg = extend_arg!(*arg);
                push!(Instruction::Store {
                    into: UnresolvedPlace::Local(arg >> 4),
                });
                push!(Instruction::Store {
                    into: UnresolvedPlace::Local(arg * 15),
                })
            }
            (116, arg) => push!(Instruction::Store {
                into: UnresolvedPlace::Name(extend_arg!(*arg))
            }),

            // Paired load + stores
            (113, arg) => {
                let arg = extend_arg!(*arg);
                push!(Instruction::Store {
                    into: UnresolvedPlace::Local(arg >> 4),
                });
                push!(Instruction::Load {
                    from: UnresolvedPlace::Local(arg & 15),
                })
            }

            // Pops
            (9 | 30 | 31, _) => {
                arg_extension = 0;
                push!(Instruction::Pop)
            }
            // Copy
            (59, arg) => push!(Instruction::Copy(extend_arg!(*arg))),
            //Swap
            (117, arg) => push!(Instruction::Swap(extend_arg!(*arg))),

            // Binary Ops
            (44, op) => push!(Instruction::BinaryOp(match extend_arg!(*op) {
                0 => BinOp::Add,
                1 => BinOp::And,
                2 => BinOp::FloorDiv,
                3 => BinOp::LShift,
                4 => BinOp::MatMul,
                5 => BinOp::Mul,
                6 => BinOp::Remainder,
                7 => BinOp::Or,
                8 => BinOp::Power,
                9 => BinOp::RShift,
                10 => BinOp::Sub,
                11 => BinOp::Div,
                12 => BinOp::Xor,
                13 => BinOp::InplaceAdd,
                14 => BinOp::InplaceAnd,
                15 => BinOp::InplaceFloorDiv,
                16 => BinOp::InplaceLShift,
                17 => BinOp::InplaceMatMul,
                18 => BinOp::InplaceMul,
                19 => BinOp::InplaceRemainder,
                20 => BinOp::InplaceOr,
                21 => BinOp::InplacePower,
                22 => BinOp::InplaceRShift,
                23 => BinOp::InplaceSub,
                24 => BinOp::InplaceDiv,
                25 => BinOp::InplaceXor,
                26 => BinOp::Subscript,
                n => return Err(IRParseError::OutOfBoundsBinOp(n)),
            })),
            // Comparison Ops
            (56, arg) => {
                let arg = extend_arg!(*arg);
                push!(Instruction::BinaryOp(match arg >> 5 {
                    0 => BinOp::Lt,
                    1 => BinOp::LtEq,
                    2 => BinOp::Eq,
                    3 => BinOp::Ne,
                    4 => BinOp::Gt,
                    5 => BinOp::GtEq,
                    _ => return Err(IRParseError::OutOfBoundsCompareOp(arg)),
                }));
                if arg & 16 != 0 {
                    push!(Instruction::Coercion(Coercion::Bool));
                }
            }
            // Is op
            (74, _) => {
                arg_extension = 0;
                push!(Instruction::BinaryOp(BinOp::Is))
            }

            // Unary Ops
            (41, _) => {
                arg_extension = 0;
                push!(Instruction::UnaryOp(UnaryOp::Negative))
            }
            (42, _) => {
                arg_extension = 0;
                push!(Instruction::UnaryOp(UnaryOp::LogicalNot))
            }
            (40, _) => {
                arg_extension = 0;
                push!(Instruction::UnaryOp(UnaryOp::Invert))
            }

            // Jumps
            (100, delta) => {
                let target = instruction_count + 2 + extend_arg!(*delta);
                if target as usize >= code.len() {
                    return Err(IRParseError::JumpPastEnd(target));
                }
                push!(Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target
                })
            }
            (101, delta) => {
                let target = instruction_count + 2 + extend_arg!(*delta);
                if target as usize >= code.len() {
                    return Err(IRParseError::JumpPastEnd(target));
                }
                out.push(Instruction::LoadConst(Constant::None));
                out.push(Instruction::BinaryOp(BinOp::Is));
                out.push(Instruction::UnaryOp(UnaryOp::LogicalNot));
                push!(Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target
                })
            }
            (102, delta) => {
                let target = instruction_count + 2 + extend_arg!(*delta);
                if target as usize >= code.len() {
                    return Err(IRParseError::JumpPastEnd(target));
                }
                out.push(Instruction::LoadConst(Constant::None));
                out.push(Instruction::BinaryOp(BinOp::Is));
                push!(Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target
                })
            }
            (103, delta) => {
                let target = instruction_count + 2 + extend_arg!(*delta);
                if target as usize >= code.len() {
                    return Err(IRParseError::JumpPastEnd(target));
                }
                out.push(Instruction::UnaryOp(UnaryOp::LogicalNot));
                push!(Instruction::Jump {
                    class: JumpClass::IfFalse,
                    target
                })
            }
            (77, delta) => {
                let target = instruction_count + 1 + extend_arg!(*delta);
                if target as usize >= code.len() {
                    return Err(IRParseError::JumpPastEnd(target));
                }
                push!(Instruction::Jump {
                    class: JumpClass::Always,
                    target
                })
            }
            (75, delta) => {
                let arg = extend_arg!(*delta);
                let Some(target) = (instruction_count + 1).checked_sub(arg) else {
                    return Err(IRParseError::JumpBeforeStart(arg - instruction_count - 1));
                };
                push!(Instruction::Jump {
                    class: JumpClass::Always,
                    target
                })
            }

            // Call
            (52, n) => {
                push!(Instruction::Call(extend_arg!(*n)))
            }

            // Return
            (35, _) => {
                arg_extension = 0;
                push!(Instruction::Return)
            }

            // Coercions
            (39, _) => {
                arg_extension = 0;
                push!(Instruction::Coercion(Coercion::Bool))
            }
            (16, _) => {
                arg_extension = 0;
                push!(Instruction::Coercion(Coercion::Iter))
            }
            (71, _) => {
                arg_extension = 0;
                push!(Instruction::Coercion(Coercion::Awaitable))
            }
            (14, _) => {
                arg_extension = 0;
                push!(Instruction::Coercion(Coercion::AsyncIter))
            }

            // Make Function
            (23, _) => {
                arg_extension = 0;
                push!(Instruction::MakeFunction)
            }

            // Extend args
            (69, n) => {
                if arg_extension > ((1 << 24) - 1) {
                    return Err(IRParseError::ArgExtendWouldOverflow(arg_extension));
                }
                arg_extension += *n as u32;
                arg_extension <<= 8;
            }

            // NOPs
            (27 | 0 | 128 | 28, _) => {
                arg_extension = 0;
            }

            (op, _) => return Err(IRParseError::NotYetImplementedInstruction(*op)),
        };
        instruction_count += 1;
    }

    // Patch the jumps to point to the new correct place
    for instr in out.iter_mut() {
        let Instruction::Jump { class: _, target } = instr else {
            continue;
        };
        let new_target = mapping.partition_point(|x| x < target);
        if new_target == mapping.len() {
            panic!(
                "Found an out of bounds jump in the patching step, which indicates a bug. target = {target}, new_target = {new_target}, mapping: {mapping:?}"
            );
        }

        *target = new_target as u32
    }

    Ok(out)
}

fn as_tuple(code: &[u8]) -> &[(u8, u8)] {
    assert!(
        code.len() % 2 == 0,
        "Since 3.6 code byte strings should be pairs of (instruction, opcode) bytes, and have even length"
    );
    // SAFETY: This is safe if we know that code.len is even. This is because
    // any two u8 adjacent forms a valid (u8, u8), and because the new slice is
    // not out of bounds of the original allocation (it's length in bytes, and
    // start address is the same)
    unsafe { std::slice::from_raw_parts(code.as_ptr() as *const (u8, u8), code.len() / 2) }
}
