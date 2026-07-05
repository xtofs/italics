pub mod builder;
pub mod constraints;
pub mod vars;
pub mod instr;
pub mod regs;
pub mod solver;
pub mod types;

pub use builder::IRBuilder;
pub use constraints::Constraint;
pub use vars::{TypeVar, TypeVarGenerator};
pub use instr::Instr;
pub use regs::{Reg, RegGenerator, RegId, RegisterFile};
pub use solver::{Solver, TypeError};
pub use types::{Existential, FuncType, Row, Type};
