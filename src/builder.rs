use crate::instr::Instr;
use crate::regs::{Reg, RegGenerator, RegisterFile};
use crate::vars::TypeVarGenerator;

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
}
