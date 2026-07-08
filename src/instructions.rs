use core::fmt;
// use std::fmt::write;

use crate::registers::Reg;
use crate::types::{FuncType, Type};

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i64),
    Bool(bool),
    /// The single value of the [`Unit`](crate::types::Type::Unit) type.
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Unit => write!(f, "()"),
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
        src: Option<Reg>,
    },
    /// Value-producing conditional; see [`If`].
    If(If),
    /// Bounded loop with a loop-carried accumulator; see [`For`].
    For(For),
}

/// A nested sub-block that runs for its side effects and *yields* the value in
/// `result` (the register the block evaluates to).
#[derive(Debug, Clone)]
pub struct Block {
    pub instructions: Vec<Instr>,
    pub result: Reg,
}

/// Value-producing conditional. Both blocks run for their side effects; the
/// branch that executes leaves its result in `dst` (the two branch results are
/// unified with `dst`, so the merge is a single `Equal` — no phi node, no
/// fixpoint).
#[derive(Debug, Clone)]
pub struct If {
    pub cond: Reg,
    pub then_: Block,
    pub else_: Block,
    pub dst: Reg,
}

/// Bounded loop with a loop-carried accumulator. `index` runs `0..bound`. `acc`
/// starts at `init` and is set to the body's yielded value after each iteration.
/// The loop invariant `acc = body.result` is *checked* (a plain `Equal`), never
/// fixpoint-inferred.
#[derive(Debug, Clone)]
pub struct For {
    pub index: Reg,
    pub bound: Reg,
    pub acc: Reg,
    pub init: Reg,
    pub body: Block,
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
            Instr::If(f) => Some(&f.dst),
            Instr::For(f) => Some(&f.acc),
        }
    }
}

// obj = { x: t_0, y: t_1 }
// load obj.x
// call f(obj, obj.x)

impl Instr {
    /// Format this instruction, indenting any nested blocks relative to `depth`.
    fn fmt_at(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let pad = "    ".repeat(depth + 1);
        match self {
            Instr::If(instr) => {
                writeln!(f, "if {} -> {} {{", instr.cond, instr.dst)?;
                for i in &instr.then_.instructions {
                    f.write_str(&pad)?;
                    i.fmt_at(f, depth + 1)?;
                    f.write_str("\n")?;
                }
                writeln!(f, "{}yield {}", pad, instr.then_.result)?;
                writeln!(f, "{}}} else {{", "    ".repeat(depth))?;
                for i in &instr.else_.instructions {
                    f.write_str(&pad)?;
                    i.fmt_at(f, depth + 1)?;
                    f.write_str("\n")?;
                }
                writeln!(f, "{}yield {}", pad, instr.else_.result)?;
                write!(f, "{}}}", "    ".repeat(depth))
            }
            Instr::For(instr) => {
                writeln!(
                    f,
                    "for {} in 0..{}, acc {} = {} {{",
                    instr.index, instr.bound, instr.acc, instr.init
                )?;
                for i in &instr.body.instructions {
                    f.write_str(&pad)?;
                    i.fmt_at(f, depth + 1)?;
                    f.write_str("\n")?;
                }
                writeln!(f, "{}yield {}", pad, instr.body.result)?;
                write!(f, "{}}}", "    ".repeat(depth))
            }
            other => write!(f, "{}", DisplayLeaf(other)),
        }
    }
}

/// Renders the non-control (single-line) instruction forms.
struct DisplayLeaf<'a>(&'a Instr);

impl fmt::Display for DisplayLeaf<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
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
            Instr::Ret { src } => match src {
                Some(r) => write!(f, "ret  {}", r),
                None => write!(f, "ret"),
            },
            // Control-flow forms are rendered by `Instr::fmt_at`; `DisplayLeaf`
            // is only ever constructed for the single-line instructions above.
            Instr::If(_) | Instr::For(_) => unreachable!("control flow uses fmt_at"),
        }
    }
}

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_at(f, 0)
    }
}
