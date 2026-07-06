use crate::instructions::{BinOpKind, Instr, Value};
use crate::program::IRFunction;
use crate::registers::{Reg, RegGenerator, RegisterFile};
use crate::types::{FuncType, Type};
use crate::variables::TypeVarGenerator;

#[derive(Debug, Default)]
pub struct IRBuilder {
    pub body: Vec<Instr>,
    pub register_file: RegisterFile,
    pub type_variable_generator: TypeVarGenerator,
    pub register_generator: RegGenerator,
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

    /// Finalize this builder into a named IR function with an explicit
    /// signature. This is the first step toward intra-IR function definitions.
    pub fn finish(mut self, name: impl Into<String>, params: Vec<Type>, ret: Type) -> IRFunction {
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
}
