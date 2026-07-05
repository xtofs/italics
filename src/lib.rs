pub mod builder;
pub mod constraints;
pub mod ids;
pub mod instr;
pub mod regs;
pub mod solver;
pub mod types;

pub use builder::IRBuilder;
pub use constraints::Constraint;
pub use ids::{TypeVar, TypeVarGenerator};
pub use instr::Instr;
pub use regs::{Reg, RegGenerator, RegId, RegisterFile};
pub use solver::{Kind, Solver, TypeError};
pub use types::{Existential, FuncType, Row, Type};
