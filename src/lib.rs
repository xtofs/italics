pub mod builder;
pub mod constraints;
pub mod variables;
pub mod instructions;
pub mod registers;
pub mod solver;
pub mod types;

pub use builder::IRBuilder;
pub use constraints::Constraint;
pub use variables::{TypeVar, TypeVarGenerator};
pub use instructions::Instr;
pub use registers::{Reg, RegGenerator, RegId, RegisterFile};
pub use solver::{Solver, TypeError};
pub use types::{Existential, FuncType, Row, Type};
