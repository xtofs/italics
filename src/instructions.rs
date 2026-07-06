use core::fmt;
// use std::fmt::write;

use crate::registers::Reg;
use crate::types::{FuncType, Type};

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i64),
    Bool(bool),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Lt, // (int, int) -> bool
}

impl BinOpKind {
    /// The C operator symbol for this binary operation.
    pub fn symbol(&self) -> &'static str {
        match self {
            BinOpKind::Add => "+",
            BinOpKind::Sub => "-",
            BinOpKind::Mul => "*",
            BinOpKind::Lt => "<",
        }
    }
}

impl fmt::Display for BinOpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}

#[derive(Debug, Clone)]
pub enum Instr {
    Load {
        dst: Reg,
        src: Reg,
        field: String,
    },
    Store {
        dst: Reg,
        field: String,
        src: Reg,
    },
    NewObj {
        dst: Reg,
        fields: Vec<(String, Reg)>,
    },
    Call {
        func: Reg,
        args: Vec<Reg>,
        ret: Reg,
    },
    Const {
        dst: Reg,
        value: Value,
    },
    BinOp {
        dst: Reg,
        op: BinOpKind,
        lhs: Reg,
        rhs: Reg,
    },
    LoadFunc {
        dst: Reg,
        name: String,
        sig: FuncType,
    },
    Ret {
        src: Reg,
    },
}

impl Instr {
    pub fn dst(&self) -> Option<&Reg> {
        match self {
            Instr::Load { dst, .. } => Some(dst),
            Instr::Store { dst, .. } => Some(dst),
            Instr::NewObj { dst, .. } => Some(dst),
            Instr::Call { ret, .. } => Some(ret), // TODO: is ret really a dst ?
            Instr::Const { dst, .. } => Some(dst),
            Instr::BinOp { dst, .. } => Some(dst),
            Instr::LoadFunc { dst, .. } => Some(dst),
            Instr::Ret { .. } => None,
        }
    }
}

// obj = { x: τ0, y: τ1 }
// load obj.x
// call f(obj, obj.x)

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instr::Load { dst, src, field } => write!(f, "load {} := {}.{}", dst, src, field),
            Instr::Store { dst, field, src } => write!(f, "store {}.{} := {}", dst, field, src),
            Instr::NewObj { dst, fields } => {
                write!(f, "new {} := {{ ", dst)?;
                for (i, item) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}: {}", item.0, item.1)?;
                }
                write!(f, " }}")?;
                Ok(())
            }
            Instr::Call { func, args, ret } => {
                write!(f, "call {} := {}(", ret, func)?;
                for (i, item) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Instr::Const { dst, value } => write!(f, "const {} := {}", dst, value),
            Instr::BinOp { dst, op, lhs, rhs } => {
                write!(f, "op {} := {} {} {}", dst, lhs, op, rhs)
            }
            Instr::LoadFunc { dst, name, sig } => {
                write!(f, "ldfn {} = @{} : {}", dst, name, Type::Func(sig.clone()))
            }
            Instr::Ret { src } => write!(f, "ret  {}", src),
        }
    }
}
