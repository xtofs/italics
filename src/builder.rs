use crate::instructions::{BinOpKind, Block, For, If, Instr, Value};
use crate::program::IRFunction;
use crate::registers::{Reg, RegGenerator, RegisterFile};
use crate::types::{FuncType, Type};
use crate::variables::TypeVarGenerator;

#[derive(Debug)]
pub struct FunctionBuilder<const N: usize> {
    name: String,
    params: [Type; N],
    ret: Type,
    param_regs: [Reg; N],
    ir: InstructionBuilder,
}

impl<const N: usize> FunctionBuilder<N> {
    pub fn new(name: impl Into<String>, params: [Type; N], ret: Type) -> Self {
        let mut ir = InstructionBuilder::default();
        // Reserve the first `N` registers (`reg0..regN`) as the parameter
        // registers — codegen binds parameters to exactly these. Done up front,
        // before any body instructions, so the ids are predictable.
        let param_regs = std::array::from_fn(|_| ir.reg());

        Self {
            name: name.into(),
            params,
            ret,
            param_regs,
            ir,
        }
    }

    /// Return the pre-declared parameter register at `index`.
    pub fn param(&self, index: usize) -> Reg {
        *self.param_regs.get(index).unwrap_or_else(|| {
            panic!(
                "FunctionBuilder::param index {} out of bounds for {} parameters",
                index, N
            )
        })
    }

    pub fn params(&self) -> [Reg; N] {
        self.param_regs
    }

    pub fn build(self) -> IRFunction {
        self.ir
            .finish(self.name, self.params.into_iter().collect(), self.ret)
    }
}

/// A `FunctionBuilder` is signature ceremony wrapped around an [`InstructionBuilder`]
/// body: it adds typed, compile-time-arity parameters plus `build`, and reuses
/// the entire IR-emitting surface via `Deref`. (`InstructionBuilder::finish` takes the
/// builder by value, so it can't be reached through `Deref` — `build` is the
/// only finalizer.)
impl<const N: usize> std::ops::Deref for FunctionBuilder<N> {
    type Target = InstructionBuilder;

    fn deref(&self) -> &InstructionBuilder {
        &self.ir
    }
}

impl<const N: usize> std::ops::DerefMut for FunctionBuilder<N> {
    fn deref_mut(&mut self) -> &mut InstructionBuilder {
        &mut self.ir
    }
}

#[derive(Debug, Default)]
pub struct InstructionBuilder {
    pub body: Vec<Instr>,
    pub register_file: RegisterFile,
    pub type_variable_generator: TypeVarGenerator,
    pub register_generator: RegGenerator,
}

impl InstructionBuilder {
    pub fn reg(&mut self) -> Reg {
        let tv = self.type_variable_generator.fresh();
        let reg = self.register_generator.fresh(tv);
        self.register_file.add(reg);
        reg
    }

    pub fn new_obj(&mut self, fields: Vec<(impl Into<String>, Reg)>) -> Reg {
        let dst = self.reg();
        let fields = fields
            .into_iter()
            .map(|(name, reg)| (name.into(), reg))
            .collect();
        self.body.push(Instr::NewObj { dst, fields });
        dst
    }

    pub fn load(&mut self, src: Reg, field: impl Into<String>) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::Load {
            dst,
            src,
            field: field.into(),
        });
        dst
    }

    pub fn store(&mut self, dst: Reg, field: impl Into<String>, src: Reg) {
        self.body.push(Instr::Store {
            dst,
            field: field.into(),
            src,
        });
    }

    pub fn call(&mut self, func: Reg, args: Vec<Reg>) -> Reg {
        let ret = self.reg();
        self.body.push(Instr::Call { func, args, ret });
        ret
    }

    pub fn const_int(&mut self, v: i64) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::Const {
            dst,
            value: Value::Int(v),
        });
        dst
    }

    pub fn const_bool(&mut self, v: bool) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::Const {
            dst,
            value: Value::Bool(v),
        });
        dst
    }

    /// Materialize the single value of the unit type.
    pub fn const_unit(&mut self) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::Const {
            dst,
            value: Value::Unit,
        });
        dst
    }

    pub fn binop(&mut self, op: BinOpKind, lhs: Reg, rhs: Reg) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::BinOp { dst, op, lhs, rhs });
        dst
    }

    /// Load a runtime prelude function (e.g. `print_int`) into a register,
    /// pulling its signature from the [`prelude`](crate::prelude) table so the
    /// caller can't get it wrong. Panics if `name` is not a prelude function.
    pub fn prelude(&mut self, name: &str) -> Reg {
        let f = crate::prelude::get(name)
            .unwrap_or_else(|| panic!("unknown prelude function {:?}", name));
        let dst = self.reg();
        self.body.push(Instr::LoadFunc {
            dst,
            name: f.name.to_string(),
            sig: f.signature(),
        });
        dst
    }

    /// Load a runtime-defined function into a register, describing its
    /// signature so the solver can constrain the argument/return types.
    pub fn func(&mut self, name: impl Into<String>, params: Vec<Type>, ret: Type) -> Reg {
        let dst = self.reg();
        let sig = FuncType {
            params,
            ret: Box::new(ret),
            stack: None,
        };
        self.body.push(Instr::LoadFunc {
            dst,
            name: name.into(),
            sig,
        });
        dst
    }

    pub fn ret(&mut self, src: Reg) {
        self.body.push(Instr::Ret { src: Some(src) });
    }

    /// Return with no explicit value — the function yields unit. Only valid in a
    /// unit-returning function.
    pub fn ret_unit(&mut self) {
        self.body.push(Instr::Ret { src: None });
    }

    /// Build a sub-block: run `f` with the body temporarily swapped for a fresh
    /// one, and return the instructions it emitted together with whatever `f`
    /// produced. The register/type-variable generators and the register file
    /// are shared throughout, so register ids stay globally unique.
    fn block_with<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (Vec<Instr>, R) {
        let saved = std::mem::take(&mut self.body);
        let result = f(self);
        (std::mem::replace(&mut self.body, saved), result)
    }

    /// Value-producing conditional. Each branch closure builds its block and
    /// returns the register holding that branch's result; the two are merged
    /// into a single `dst` register, returned to the caller. Only that `dst`
    /// escapes the blocks, so block-local registers cannot leak.
    pub fn if_value(
        &mut self,
        cond: Reg,
        then_f: impl FnOnce(&mut Self) -> Reg,
        else_f: impl FnOnce(&mut Self) -> Reg,
    ) -> Reg {
        let (then_instrs, then_result) = self.block_with(then_f);
        let (else_instrs, else_result) = self.block_with(else_f);
        let dst = self.reg();
        self.body.push(Instr::If(If {
            cond,
            then_: Block {
                instrs: then_instrs,
                result: then_result,
            },
            else_: Block {
                instrs: else_instrs,
                result: else_result,
            },
            dst,
        }));
        dst
    }

    /// Bounded loop with a loop-carried accumulator. The body closure receives
    /// the induction variable (`0..bound`) and the current accumulator, and
    /// returns the register holding the accumulator's next value. The final
    /// accumulator register is returned to the caller.
    pub fn for_acc(
        &mut self,
        bound: Reg,
        init: Reg,
        body_f: impl FnOnce(&mut Self, Reg, Reg) -> Reg,
    ) -> Reg {
        // Allocate index/acc up front so the body can reference them; `reg`
        // only allocates a register, it emits no instruction.
        let index = self.reg();
        let acc = self.reg();
        let (instrs, next) = self.block_with(|b| body_f(b, index, acc));
        self.body.push(Instr::For(For {
            index,
            bound,
            acc,
            init,
            body: Block {
                instrs,
                result: next,
            },
        }));
        acc
    }

    /// Finalize this builder into a named IR function with an explicit
    /// signature. This is the first step toward intra-IR function definitions.
    pub fn finish(mut self, name: impl Into<String>, params: Vec<Type>, ret: Type) -> IRFunction {
        // Reserve predictable parameter registers [reg0..regN) even when the
        // caller did not allocate them explicitly (`FunctionBuilder` always does,
        // but a bare `InstructionBuilder` finished with a signature may not have).
        while self.register_file.len() < params.len() {
            let _ = self.reg();
        }

        IRFunction::new(name, params, ret, self.body, self.register_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reserves_missing_parameter_registers() {
        let b = InstructionBuilder::default();
        let f = b.finish("f", vec![Type::Int, Type::Int], Type::Int);

        let regs: Vec<_> = f.registers.iter().collect();
        assert!(regs.len() >= 2);
        assert_eq!(regs[0].id.0, 0);
        assert_eq!(regs[1].id.0, 1);
    }

    #[test]
    fn function_builder_param_access_matches_declared_signature() {
        let b = FunctionBuilder::new("f", [Type::Int, Type::Bool], Type::Int);

        assert_eq!(b.param(0).id.0, 0);
        assert_eq!(b.param(1).id.0, 1);
    }

    #[test]
    #[should_panic(expected = "FunctionBuilder::param index 2 out of bounds for 2 parameters")]
    fn function_builder_panics_on_out_of_bounds_param() {
        let b = FunctionBuilder::new("f", [Type::Int, Type::Bool], Type::Int);

        let _ = b.param(2);
    }

    #[test]
    fn function_builder_params_array_has_compile_time_arity() {
        let b = FunctionBuilder::new("f", [Type::Int, Type::Bool], Type::Int);
        let [p0, p1] = b.params();

        assert_eq!(p0.id.0, 0);
        assert_eq!(p1.id.0, 1);
    }
}
