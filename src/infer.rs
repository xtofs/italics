//! The type-inference → code-generation pipeline, expressed as type-state
//! stages: `Inference` → `Constraints` → `Solved` → C source.
//!
//! Each stage carries its own result as a public field and offers a
//! `self`-consuming method to the next stage, so the whole run reads
//! `Inference::new(..).generate_constraints(..).solve(..)?.generate_code()?`
//! while intermediate results stay inspectable one stage at a time. Advancing
//! moves the stage, so a mis-ordered call is a compile error and nothing re-runs.

use std::collections::BTreeMap;

use crate::codegen::{self, CodegenError};
use crate::constraints::Constraint;
use crate::instructions::{BinOpKind, Instr, Value};
use crate::program::Function;
use crate::registers::RegisterFile;
use crate::solver::{Solver, TypeError};
use crate::types::{FuncType, Row, Type};
use crate::variables::TypeVarGenerator;

/// Constraints for a single instruction, recursing into control-flow sub-blocks.
/// Needs `tvg` only to mint the fresh row tail a `NewObj` opens.
pub fn constraints_from_instr(instr: &Instr, tvg: &mut TypeVarGenerator) -> Vec<Constraint> {
    match instr {
        Instr::Load { dst, src, field } => vec![
            Constraint::RowHasField(src.ty(), field.clone()),
            Constraint::RowFieldType(src.ty(), field.clone(), dst.ty()),
        ],

        Instr::Store { dst, field, src } => vec![
            Constraint::RowHasField(dst.ty(), field.clone()),
            Constraint::RowFieldType(dst.ty(), field.clone(), src.ty()),
        ],

        Instr::NewObj { dst, fields } => {
            // Create a fresh row tail type variable so the object is open.
            let tail_tv = tvg.fresh_row();
            let mut row = Row {
                fields: BTreeMap::new(),
                tail: Some(tail_tv),
            };
            for (name, reg) in fields {
                row.fields.insert(name.clone(), reg.ty());
            }
            vec![Constraint::Equal(dst.ty(), Type::Record(row))]
        }

        Instr::Call { func, args, ret } => {
            let func_ty = Type::Func(FuncType {
                params: args.iter().map(|r| r.ty()).collect(),
                ret: Box::new(ret.ty()),
                stack: None,
            });
            vec![Constraint::Equal(func.ty(), func_ty)]
        }

        Instr::Const { dst, value } => {
            let ty = match value {
                Value::Int(_) => Type::Int,
                Value::Bool(_) => Type::Bool,
                Value::Unit => Type::Unit,
            };
            vec![Constraint::Equal(dst.ty(), ty)]
        }

        Instr::BinOp { dst, op, lhs, rhs } => {
            // operands are always ints; the result is bool for comparisons
            let dst_ty = match op {
                BinOpKind::Lt => Type::Bool,
                _ => Type::Int,
            };
            vec![
                Constraint::Equal(lhs.ty(), Type::Int),
                Constraint::Equal(rhs.ty(), Type::Int),
                Constraint::Equal(dst.ty(), dst_ty),
            ]
        }

        Instr::LoadFunc { dst, name: _, sig } => {
            // The runtime function's declared signature enters the constraint
            // system and unifies with the func type a later Call synthesizes, so
            // argument/return types flow both ways.
            vec![Constraint::Equal(dst.ty(), Type::Func(sig.clone()))]
        }

        Instr::Ret { .. } => {
            // The returned value's type flows from the body; it is bound to the
            // function's declared return type by `Inference::for_function`, not
            // here.
            vec![]
        }

        Instr::If(f) => {
            let mut cs = vec![Constraint::Equal(f.cond.ty(), Type::Bool)];
            for instr in &f.then_.instrs {
                cs.extend(constraints_from_instr(instr, tvg));
            }
            for instr in &f.else_.instrs {
                cs.extend(constraints_from_instr(instr, tvg));
            }
            // Merge: whichever branch runs, its result flows into `dst`. Pushed
            // after the block constraints so the stable weight-sort keeps
            // "result defined before merged".
            cs.push(Constraint::Equal(f.dst.ty(), f.then_.result.ty()));
            cs.push(Constraint::Equal(f.dst.ty(), f.else_.result.ty()));
            cs
        }

        Instr::For(f) => {
            let mut cs = vec![
                Constraint::Equal(f.index.ty(), Type::Int),
                Constraint::Equal(f.bound.ty(), Type::Int),
                // Accumulator is seeded from `init`.
                Constraint::Equal(f.acc.ty(), f.init.ty()),
            ];
            for instr in &f.body.instrs {
                cs.extend(constraints_from_instr(instr, tvg));
            }
            // Checked loop invariant: the body's yielded value must have the same
            // type as the accumulator (a plain `Equal`, not a fixpoint).
            cs.push(Constraint::Equal(f.acc.ty(), f.body.result.ty()));
            cs
        }
    }
}

/// Constraints for a whole instruction body.
pub fn constraints_for(body: &[Instr], tvg: &mut TypeVarGenerator) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    for instr in body {
        constraints.extend(constraints_from_instr(instr, tvg));
    }
    constraints
}

/// Pipeline entry: a body plus the register file it types.
pub struct Inference<'a> {
    body: &'a [Instr],
    registers: &'a RegisterFile,
    seed: Vec<Constraint>,
}

impl<'a> Inference<'a> {
    pub fn new(body: &'a [Instr], registers: &'a RegisterFile) -> Self {
        Self {
            body,
            registers,
            seed: Vec::new(),
        }
    }

    /// Test escape hatch: pre-load constraints before generation. Not part of
    /// the normal flow — the only real "extra" constraints (a function's
    /// signature binding) are produced by [`for_function`](Self::for_function).
    pub fn seed(mut self, extra: Vec<Constraint>) -> Self {
        self.seed = extra;
        self
    }

    /// Generate the body's constraints.
    pub fn generate_constraints(self, tvg: &mut TypeVarGenerator) -> Constraints<'a> {
        let mut constraints = self.seed;
        constraints.extend(constraints_for(self.body, tvg));
        Constraints {
            constraints,
            body: self.body,
            registers: self.registers,
        }
    }

    /// Generate a function's constraints: its body plus the param/ret bindings
    /// that tie it to the declared signature, in one pass (the bindings are
    /// *generated*, never appended after the fact).
    pub fn for_function(function: &'a Function, tvg: &mut TypeVarGenerator) -> Constraints<'a> {
        let mut constraints = Vec::new();

        // Parameters: the first N registers carry the declared parameter types.
        for (reg, param_ty) in function
            .registers
            .iter()
            .take(function.signature.params.len())
            .zip(function.signature.params.iter())
        {
            constraints.push(Constraint::Equal(reg.ty(), param_ty.clone()));
        }

        constraints.extend(constraints_for(&function.body, tvg));

        // Each `ret` is bound to the declared return type; a valueless `ret`
        // requires the function to return unit.
        let ret_ty = (*function.signature.ret).clone();
        for instr in &function.body {
            if let Instr::Ret { src } = instr {
                let returned = match src {
                    Some(r) => r.ty(),
                    None => Type::Unit,
                };
                constraints.push(Constraint::Equal(returned, ret_ty.clone()));
            }
        }

        Constraints {
            constraints,
            body: &function.body,
            registers: &function.registers,
        }
    }
}

/// Stage 1 result: the generated constraints (read-only), plus the context the
/// later stages need.
pub struct Constraints<'a> {
    pub constraints: Vec<Constraint>,
    body: &'a [Instr],
    registers: &'a RegisterFile,
}

impl<'a> Constraints<'a> {
    /// Solve the constraints, yielding inferred register types.
    pub fn solve<'t>(self, tvg: &'t mut TypeVarGenerator) -> Result<Solved<'a, 't>, TypeError> {
        let mut solver = Solver::new(tvg);
        solver.solve(&self.constraints)?;
        Ok(Solved {
            solver,
            body: self.body,
            registers: self.registers,
        })
    }
}

/// Stage 2 result: the solved [`Solver`] (inspect via `solver.apply(reg.ty())`
/// / `solver.substitutions`), plus the context codegen needs.
pub struct Solved<'a, 't> {
    pub solver: Solver<'t>,
    body: &'a [Instr],
    registers: &'a RegisterFile,
}

impl Solved<'_, '_> {
    /// Lower the solved body to a runnable C translation unit.
    pub fn generate_code(self) -> Result<String, CodegenError> {
        codegen::emit_body(self.body, self.registers, &self.solver)
    }
}
