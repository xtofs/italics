use core::fmt;

use crate::types::Type;
use crate::vars::TypeVar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg {
    pub id: RegId,
    pub ty: TypeVar,
}

impl Reg {
    pub fn ty(&self) -> Type {
        Type::Unknown(self.ty)
    }
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "reg{}", self.id.0)
    }
}

#[derive(Debug, Default)]
pub struct RegGenerator {
    next: u32,
}

impl RegGenerator {
    // pub fn new() -> Self {
    //     Self { next: 0 }
    // }

    pub fn fresh(&mut self, tv: TypeVar) -> Reg {
        let id = RegId(self.next);
        self.next += 1;
        Reg { id, ty: tv }
    }
}

#[derive(Debug, Default)]
pub struct RegisterFile {
    regs: Vec<Reg>,
}

impl RegisterFile {
    pub fn add(&mut self, reg: Reg) {
        self.regs.push(reg);
    }

    pub fn iter(&self) -> impl Iterator<Item = Reg> + '_ {
        self.regs.iter().copied()
    }
}
