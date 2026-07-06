pub mod builder;
pub mod codegen;
pub mod constraints;
pub mod instructions;
pub mod registers;
pub mod solver;
pub mod types;
pub mod variables;

pub use builder::IRBuilder;
pub use codegen::{CodegenError, emit_c};
pub use constraints::Constraint;
pub use instructions::Instr;
pub use registers::{Reg, RegGenerator, RegId, RegisterFile};
pub use solver::{Solver, TypeError};
pub use types::{Existential, FuncType, Row, Type};
pub use variables::{TypeVar, TypeVarGenerator};
