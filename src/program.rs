use crate::instructions::Instr;
use crate::registers::RegisterFile;
use crate::types::{FuncType, Type};
use crate::variables::{TypeVar, TypeVarGenerator};

#[derive(Debug)]
pub struct IRFunction {
    pub name: String,
    pub signature: FuncType,
    pub body: Vec<Instr>,
    pub registers: RegisterFile,
}

impl IRFunction {
    pub fn new(
        name: impl Into<String>,
        params: Vec<Type>,
        ret: Type,
        body: Vec<Instr>,
        registers: RegisterFile,
    ) -> Self {
        Self {
            name: name.into(),
            signature: FuncType {
                params,
                ret: Box::new(ret),
                stack: None,
            },
            body,
            registers,
        }
    }
}

#[derive(Debug)]
pub struct IRProgram {
    pub functions: Vec<IRFunction>,
    pub entry: String,
}

impl IRProgram {
    pub fn new(entry: impl Into<String>) -> Self {
        Self {
            functions: Vec::new(),
            entry: entry.into(),
        }
    }

    pub fn add_function(&mut self, function: IRFunction) {
        self.functions.push(function);
    }

    pub fn function(&self, name: &str) -> Option<&IRFunction> {
        self.functions.iter().find(|f| f.name == name)
    }
}

fn collect_type_vars(ty: &Type, max_type: &mut u32, max_row: &mut u32) {
    match ty {
        Type::Int | Type::Bool => {}
        Type::Ptr(inner) => collect_type_vars(inner, max_type, max_row),
        Type::Func(func) => {
            for param in &func.params {
                collect_type_vars(param, max_type, max_row);
            }
            collect_type_vars(&func.ret, max_type, max_row);
            if let Some(stack) = &func.stack {
                for ty in stack {
                    collect_type_vars(ty, max_type, max_row);
                }
            }
        }
        Type::Record(row) | Type::Interface(row) => {
            for field_ty in row.fields.values() {
                collect_type_vars(field_ty, max_type, max_row);
            }
            if let Some(tail) = row.tail {
                if tail.is_row() {
                    *max_row = (*max_row).max(tail.index());
                }
            }
        }
        Type::Existential(existential) => {
            if existential.var.is_row() {
                *max_row = (*max_row).max(existential.var.index());
            } else {
                *max_type = (*max_type).max(existential.var.index());
            }
            collect_type_vars(&existential.ty, max_type, max_row);
        }
        Type::Stack(types) => {
            for ty in types {
                collect_type_vars(ty, max_type, max_row);
            }
        }
        Type::Unknown(tv) => {
            if tv.is_row() {
                *max_row = (*max_row).max(tv.index());
            } else {
                *max_type = (*max_type).max(tv.index());
            }
        }
    }
}

fn collect_func_type_vars(func: &FuncType, max_type: &mut u32, max_row: &mut u32) {
    for param in &func.params {
        collect_type_vars(param, max_type, max_row);
    }
    collect_type_vars(&func.ret, max_type, max_row);
    if let Some(stack) = &func.stack {
        for ty in stack {
            collect_type_vars(ty, max_type, max_row);
        }
    }
}

fn collect_var(tv: TypeVar, max_type: &mut u32, max_row: &mut u32) {
    if tv.is_row() {
        *max_row = (*max_row).max(tv.index());
    } else {
        *max_type = (*max_type).max(tv.index());
    }
}

/// Create a fresh type-variable generator that starts *after* every type
/// variable referenced by the function's signature, body and register file.
///
/// This keeps solver-introduced variables disjoint from variables already
/// present in the IR, even when a function is solved independently.
pub fn type_var_generator_for_function(function: &IRFunction) -> TypeVarGenerator {
    let mut max_type = 0_u32;
    let mut max_row = 0_u32;

    collect_func_type_vars(&function.signature, &mut max_type, &mut max_row);

    for reg in function.registers.iter() {
        collect_var(reg.tv, &mut max_type, &mut max_row);
    }

    for instr in &function.body {
        match instr {
            Instr::Load { dst, src, .. } => {
                collect_var(dst.tv, &mut max_type, &mut max_row);
                collect_var(src.tv, &mut max_type, &mut max_row);
            }
            Instr::Store { dst, src, .. } => {
                collect_var(dst.tv, &mut max_type, &mut max_row);
                collect_var(src.tv, &mut max_type, &mut max_row);
            }
            Instr::NewObj { dst, fields } => {
                collect_var(dst.tv, &mut max_type, &mut max_row);
                for (_, reg) in fields {
                    collect_var(reg.tv, &mut max_type, &mut max_row);
                }
            }
            Instr::Call { func, args, ret } => {
                collect_var(func.tv, &mut max_type, &mut max_row);
                collect_var(ret.tv, &mut max_type, &mut max_row);
                for arg in args {
                    collect_var(arg.tv, &mut max_type, &mut max_row);
                }
            }
            Instr::Const { dst, .. } => collect_var(dst.tv, &mut max_type, &mut max_row),
            Instr::BinOp { dst, lhs, rhs, .. } => {
                collect_var(dst.tv, &mut max_type, &mut max_row);
                collect_var(lhs.tv, &mut max_type, &mut max_row);
                collect_var(rhs.tv, &mut max_type, &mut max_row);
            }
            Instr::LoadFunc { dst, sig, .. } => {
                collect_var(dst.tv, &mut max_type, &mut max_row);
                collect_func_type_vars(sig, &mut max_type, &mut max_row);
            }
            Instr::Ret { src } => collect_var(src.tv, &mut max_type, &mut max_row),
        }
    }

    TypeVarGenerator::new(max_type.saturating_add(1), max_row.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registers::RegisterFile;
    use crate::variables::TypeVar;

    #[test]
    fn program_stores_and_finds_functions() {
        let mut program = IRProgram::new("main");
        let fun = IRFunction::new(
            "main",
            vec![Type::Int],
            Type::Int,
            Vec::new(),
            RegisterFile::default(),
        );
        program.add_function(fun);

        assert!(program.function("main").is_some());
        assert!(program.function("missing").is_none());
        assert_eq!(program.entry, "main");
    }

    #[test]
    fn seeded_generator_starts_after_used_function_vars() {
        let fun = IRFunction::new(
            "main",
            vec![Type::Unknown(TypeVar(5))],
            Type::Unknown(TypeVar(9)),
            Vec::new(),
            RegisterFile::default(),
        );

        let mut tvg = type_var_generator_for_function(&fun);
        let next = tvg.fresh();
        assert!(next.index() >= 10);
    }
}
