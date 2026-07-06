use core::fmt;

use crate::types::Type;
use crate::variables::TypeVar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reg {
    pub id: RegId,
    pub tv: TypeVar,
}

impl Reg {
    pub fn ty(&self) -> Type {
        Type::Unknown(self.tv)
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
        Reg { id, tv }
    }
}

#[derive(Debug, Default)]
pub struct RegisterFile {
    registers: Vec<Reg>,
}

impl RegisterFile {
    pub fn add(&mut self, reg: Reg) {
        self.registers.push(reg);
    }

    pub fn iter(&self) -> impl Iterator<Item = Reg> + '_ {
        self.registers.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.registers.len()
    }

    pub fn get(&self, index: usize) -> Option<Reg> {
        self.registers.get(index).copied()
    }
}
