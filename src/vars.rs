#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar(pub u32);

impl TypeVar {
    pub fn is_row(&self) -> bool {
        self.0 & 0x8000_0000 != 0
    }
}

// IR Builder → generates TypeVars
// Constraint Generator → uses TypeVars
// Solver → binds TypeVars

#[derive(Debug)]
pub struct TypeVarGenerator {
    next: u32,
    next_row: u32,
}

impl TypeVarGenerator {
    pub fn new() -> Self {
        Self {
            next: 0,
            next_row: 0,
        }
    }

    pub fn fresh(&mut self) -> TypeVar {
        let v = TypeVar(self.next);
        self.next += 1;

        if (self.next & 0x8000_0000) != 0 {
            panic!("type var id overflow")
        }

        v
    }

    pub fn fresh_row(&mut self) -> TypeVar {
        let v = TypeVar(self.next_row | 0x8000_0000);
        self.next_row += 1;

        if (self.next_row & 0x8000_0000) != 0 {
            panic!("row type var id overflow")
        }
        v
    }
}
