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
    ir: IRBuilder,
}

impl<const N: usize> FunctionBuilder<N> {
    pub fn new(name: impl Into<String>, params: [Type; N], ret: Type) -> Self {
        let mut ir = IRBuilder::default();
        let param_regs = std::array::from_fn(|index| ir.param(index));

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

    pub fn reg(&mut self) -> Reg {
        self.ir.reg()
    }

    pub fn new_obj(&mut self, fields: Vec<(impl Into<String>, Reg)>) -> Reg {
        self.ir.new_obj(fields)
    }

    pub fn load(&mut self, src: Reg, field: impl Into<String>) -> Reg {
        self.ir.load(src, field)
    }

    pub fn store(&mut self, dst: Reg, field: impl Into<String>, src: Reg) {
        self.ir.store(dst, field, src);
    }

    pub fn call(&mut self, func: Reg, args: Vec<Reg>) -> Reg {
        self.ir.call(func, args)
    }

    pub fn const_int(&mut self, v: i64) -> Reg {
        self.ir.const_int(v)
    }

    pub fn const_bool(&mut self, v: bool) -> Reg {
        self.ir.const_bool(v)
    }

    pub fn binop(&mut self, op: BinOpKind, lhs: Reg, rhs: Reg) -> Reg {
        self.ir.binop(op, lhs, rhs)
    }

    pub fn func(&mut self, name: impl Into<String>, params: Vec<Type>, ret: Type) -> Reg {
        self.ir.func(name, params, ret)
    }

    pub fn ret(&mut self, src: Reg) {
        self.ir.ret(src);
    }

    /// Value-producing conditional. Branch bodies are built against the inner
    /// `IRBuilder` (parameters are already allocated, so a block body only needs
    /// the value-building methods).
    pub fn if_value(
        &mut self,
        cond: Reg,
        then_f: impl FnOnce(&mut IRBuilder) -> Reg,
        else_f: impl FnOnce(&mut IRBuilder) -> Reg,
    ) -> Reg {
        self.ir.if_value(cond, then_f, else_f)
    }

    /// Bounded loop with a loop-carried accumulator; see [`IRBuilder::for_acc`].
    pub fn for_acc(
        &mut self,
        bound: Reg,
        init: Reg,
        body_f: impl FnOnce(&mut IRBuilder, Reg, Reg) -> Reg,
    ) -> Reg {
        self.ir.for_acc(bound, init, body_f)
    }

    pub fn build(self) -> IRFunction {
        self.ir
            .finish(self.name, self.params.into_iter().collect(), self.ret)
    }
}

#[derive(Debug, Default)]
pub struct IRBuilder {
    pub body: Vec<Instr>,
    pub register_file: RegisterFile,
    pub type_variable_generator: TypeVarGenerator,
    pub register_generator: RegGenerator,
    pub max_param_index: Option<usize>,
}

impl IRBuilder {
    // pub fn new() -> Self {
    //     Self {
    //         type_variable_generator: TypeVarGenerator::default(),
    //         register_generator: RegGenerator::default(),
    //         register_file: RegisterFile::default(),
    //         body: Vec::new(),
    //     }
    // }

    pub fn reg(&mut self) -> Reg {
        let tv = self.type_variable_generator.fresh();
        let reg = self.register_generator.fresh(tv);
        self.register_file.add(reg);
        reg
    }

    /// Return the parameter register at `index`, reserving parameter registers
    /// from `reg0` upward as needed.
    pub fn param(&mut self, index: usize) -> Reg {
        self.max_param_index = Some(
            self.max_param_index
                .map_or(index, |current| current.max(index)),
        );
        while self.register_file.len() <= index {
            let _ = self.reg();
        }
        self.register_file
            .get(index)
            .expect("parameter register index must exist after reservation")
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

    pub fn binop(&mut self, op: BinOpKind, lhs: Reg, rhs: Reg) -> Reg {
        let dst = self.reg();
        self.body.push(Instr::BinOp { dst, op, lhs, rhs });
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
        self.body.push(Instr::Ret { src });
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
        if let Some(max_param_index) = self.max_param_index {
            assert!(
                params.len() > max_param_index,
                "IRBuilder::finish declared {} params, but highest referenced parameter index is {}",
                params.len(),
                max_param_index
            );
        }

        // Reserve predictable parameter registers [reg0..regN) even when the
        // caller did not request them explicitly during IR construction.
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
    fn param_reserves_predictable_register_id() {
        let mut b = IRBuilder::default();
        let p0 = b.param(0);
        let p2 = b.param(2);

        assert_eq!(p0.id.0, 0);
        assert_eq!(p2.id.0, 2);
    }

    #[test]
    fn finish_reserves_missing_parameter_registers() {
        let b = IRBuilder::default();
        let f = b.finish("f", vec![Type::Int, Type::Int], Type::Int);

        let regs: Vec<_> = f.registers.iter().collect();
        assert!(regs.len() >= 2);
        assert_eq!(regs[0].id.0, 0);
        assert_eq!(regs[1].id.0, 1);
    }

    #[test]
    #[should_panic(
        expected = "IRBuilder::finish declared 1 params, but highest referenced parameter index is 1"
    )]
    fn finish_panics_when_referenced_param_exceeds_signature() {
        let mut b = IRBuilder::default();
        let _ = b.param(1);

        let _ = b.finish("f", vec![Type::Int], Type::Int);
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
