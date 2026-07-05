#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar(pub u32);

// IR Builder → generates TypeVars
// Constraint Generator → uses TypeVars
// Solver → binds TypeVars

#[derive(Debug)]
pub struct TypeVarGenerator {
    next: u32,
}

impl TypeVarGenerator {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    pub fn fresh(&mut self) -> TypeVar {
        let v = TypeVar(self.next);
        self.next += 1;
        v
    }
}
