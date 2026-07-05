use core::fmt;
// use std::fmt::write;

use crate::regs::Reg;

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
}

// obj = { x: τ0, y: τ1 }
// load obj.x
// call f(obj, obj.x)

impl fmt::Display for Instr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instr::Load { dst, src, field } => write!(f, "load {} = {}.{}", dst, src, field),
            Instr::Store { dst, field, src } => write!(f, "store {}.{} = {}", dst, field, src),
            Instr::NewObj { dst, fields } => {
                write!(f, "new  {} = {{ ", dst)?;
                for (i, item) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}: {}", item.0, item.1)?;
                }
                write!(f, "}}")?;
                Ok(())
            }
            Instr::Call { func, args, ret } => {
                write!(f, "call {} = {}(", ret, func)?;
                for (i, item) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
        }
    }
}
