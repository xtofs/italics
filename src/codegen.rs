use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;

use crate::indenting::IndentedWriter;
use crate::instructions::Instr;
use crate::program::{Function, Program, type_var_generator_for_function};
use crate::registers::{RegId, RegisterFile};
use crate::solver::{Solver, TypeError};
use crate::types::{FuncType, Type};

/// The runtime preamble: standard includes plus the `unit_t` singleton
/// type that [`Type::Unit`] lowers to. An unused file-scope typedef is
/// warning-clean under `-Wall`, so it is always emitted.
const PREAMBLE: &str = "\
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

// unit type definition and it's value
typedef struct { char _; } unit_t;
static const unit_t UNIT = {0};

";

#[derive(Debug)]
pub enum CodegenError {
    /// A register's type never got resolved by inference — emitting C would
    /// mean silently defaulting it, which would hide the inference gap.
    UnresolvedType(String),
    /// A register has a type the C backend does not lower (interface,
    /// existential, stack, …).
    Unsupported(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodegenError::UnresolvedType(s) => write!(f, "unresolved type: {}", s),
            CodegenError::Unsupported(s) => write!(f, "unsupported type: {}", s),
        }
    }
}

impl std::error::Error for CodegenError {}

#[derive(Debug)]
pub enum CompilerError {
    MissingEntry(String),
    DuplicateFunction(String),
    /// A program-level construct the backend doesn't support (e.g. an entry
    /// function with parameters). Distinct from [`CodegenError::Unsupported`],
    /// which is about unsupported *types*.
    UnsupportedProgram(String),
    Type(TypeError),
    Codegen(CodegenError),
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilerError::MissingEntry(name) => {
                write!(f, "entry function {:?} was not found", name)
            }
            CompilerError::DuplicateFunction(name) => {
                write!(f, "duplicate function name {:?}", name)
            }
            CompilerError::UnsupportedProgram(msg) => {
                write!(f, "unsupported program construct: {}", msg)
            }
            CompilerError::Type(err) => write!(f, "type error: {:?}", err),
            CompilerError::Codegen(err) => write!(f, "codegen error: {}", err),
        }
    }
}

impl std::error::Error for CompilerError {}

impl From<TypeError> for CompilerError {
    fn from(value: TypeError) -> Self {
        CompilerError::Type(value)
    }
}

impl From<CodegenError> for CompilerError {
    fn from(value: CodegenError) -> Self {
        CompilerError::Codegen(value)
    }
}

/// A structurally-deduplicated record layout, emitted as `struct R<id>`.
struct StructDef {
    id: usize,
    fields: Vec<(String, String)>, // (field name, C type)
    /// The open row tail that was closed at codegen time, if any — recorded
    /// so the emitted struct can note where its width was frozen.
    closed_from: Option<String>,
}

/// A deduplicated function-pointer signature, emitted as `typedef … fn<id>`.
struct FnDef {
    id: usize,
    params: Vec<String>,
    ret: String,
}

/// A function's lowered C signature: the mangled symbol name plus the return
/// and parameter types as C strings. This is the C-level view of an
/// [`Function`]'s `(name, signature)` — derived once (it needs the solver to
/// lower types and the program to mangle names, neither of which the IR itself
/// has) and then carried through every emission site.
struct CSignature {
    name: String,
    ret: String,
    params: Vec<String>,
}

impl CSignature {
    /// The C parameter-type list: `void` when empty, else `t0, t1, …`.
    fn param_list(&self) -> String {
        if self.params.is_empty() {
            "void".to_string()
        } else {
            self.params.join(", ")
        }
    }
}

struct CodeGen<'a, 'b> {
    solver: &'a Solver<'b>,
    name_prefix: String,
    structs: Vec<StructDef>,
    struct_index: HashMap<Vec<(String, String)>, usize>,
    fns: Vec<FnDef>,
    fn_index: HashMap<(Vec<String>, String), usize>,
    reg_ctype: HashMap<RegId, String>,
}

#[derive(Clone, Copy)]
enum ReturnStyle {
    /// The `int main(void)` entry wrapper: print the result and exit `0`. The
    /// program's value is observed on stdout, never smuggled through the process
    /// exit status (which is only 8 bits and conflates result with success).
    Main,
    /// An ordinary function: return the value to its caller.
    Function,
}

impl<'a, 'b> CodeGen<'a, 'b> {
    fn new(solver: &'a Solver<'b>) -> Self {
        Self::with_prefix(solver, String::new())
    }

    fn with_prefix(solver: &'a Solver<'b>, name_prefix: String) -> Self {
        Self {
            solver,
            name_prefix,
            structs: Vec::new(),
            struct_index: HashMap::new(),
            fns: Vec::new(),
            fn_index: HashMap::new(),
            reg_ctype: HashMap::new(),
        }
    }

    fn struct_name(&self, id: usize) -> String {
        if self.name_prefix.is_empty() {
            format!("R{}", id)
        } else {
            format!("R_{}{}", self.name_prefix, id)
        }
    }

    fn fn_name(&self, id: usize) -> String {
        if self.name_prefix.is_empty() {
            format!("fn{}", id)
        } else {
            format!("fn_{}{}", self.name_prefix, id)
        }
    }

    /// Lower an inferred `Type` to a C type string, interning any records and
    /// function-pointer signatures it encounters. Fields are lowered
    /// depth-first, so nested structs/typedefs are interned before their
    /// containers.
    fn lower(&mut self, ty: &Type) -> Result<String, CodegenError> {
        match ty {
            Type::Int => Ok("int64_t".to_string()),
            Type::Bool => Ok("bool".to_string()),
            Type::Unit => Ok("unit_t".to_string()),
            Type::Ptr(inner) => Ok(format!("{}*", self.lower(inner)?)),
            Type::Record(row) => {
                let mut fields = Vec::with_capacity(row.fields.len());
                for (name, fty) in &row.fields {
                    fields.push((name.clone(), self.lower(fty)?));
                }
                let closed_from = row.tail.map(|tv| format!("{}", tv));
                let id = self.intern_struct(fields, closed_from);
                Ok(format!("struct {} *", self.struct_name(id)))
            }
            Type::Func(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in &ft.params {
                    params.push(self.lower(p)?);
                }
                let ret = self.lower(&ft.ret)?;
                let id = self.intern_fn(params, ret);
                Ok(self.fn_name(id))
            }
            Type::Unknown(tv) => Err(CodegenError::UnresolvedType(format!("{}", tv))),
            Type::Interface(_) => Err(CodegenError::Unsupported("interface".to_string())),
            Type::Existential(_) => Err(CodegenError::Unsupported("existential".to_string())),
            Type::Stack(_) => Err(CodegenError::Unsupported("stack".to_string())),
        }
    }

    /// Lower an IR function's `(name, signature)` into its C signature, applying
    /// the solver to each parameter/return type and interning any records or
    /// function pointers they mention. `name` is the already-mangled C symbol.
    fn lower_signature(
        &mut self,
        name: String,
        signature: &FuncType,
    ) -> Result<CSignature, CodegenError> {
        let mut params = Vec::with_capacity(signature.params.len());
        for param in &signature.params {
            let ty = self.solver.apply(param.clone());
            params.push(self.lower(&ty)?);
        }
        let ret_ty = self.solver.apply((*signature.ret).clone());
        let ret = self.lower(&ret_ty)?;
        Ok(CSignature { name, ret, params })
    }

    /// get or add the described struct to the struct index
    fn intern_struct(
        &mut self,
        fields: Vec<(String, String)>,
        closed_from: Option<String>,
    ) -> usize {
        if let Some(&id) = self.struct_index.get(&fields) {
            return id;
        }
        let id = self.structs.len();
        self.struct_index.insert(fields.clone(), id);
        self.structs.push(StructDef {
            id,
            fields,
            closed_from,
        });
        id
    }

    /// get or add the described function to the function index
    fn intern_fn(&mut self, params: Vec<String>, ret: String) -> usize {
        let key = (params.clone(), ret.clone());
        if let Some(&id) = self.fn_index.get(&key) {
            return id;
        }
        let id = self.fns.len();
        self.fn_index.insert(key, id);
        self.fns.push(FnDef { id, params, ret });
        id
    }

    /// Pass 1: ground and lower every register's type, populating the
    /// interning tables and the per-register C type map.
    fn ground_registers(&mut self, registers: &RegisterFile) -> Result<(), CodegenError> {
        for reg in registers.iter() {
            let ty = self.solver.apply(reg.ty());
            let ctype = match self.lower(&ty) {
                Ok(c) => c,
                Err(CodegenError::UnresolvedType(v)) => {
                    return Err(CodegenError::UnresolvedType(format!(
                        "{} has unresolved type {}",
                        reg, v
                    )));
                }
                Err(e) => return Err(e),
            };
            self.reg_ctype.insert(reg.id, ctype);
        }
        Ok(())
    }

    fn ctype(&self, id: RegId) -> &str {
        &self.reg_ctype[&id]
    }

    /// Pass 3: emit the full C translation unit.
    fn emit(&self, body: &[Instr]) -> Result<String, CodegenError> {
        let mut out = String::new();

        out.push_str(PREAMBLE);

        self.emit_supporting_type_defs(&mut out);

        // Prelude runtime functions — only the ones the program actually loads,
        // so the translation unit stays warning-clean under -Wall.
        emit_prelude(&loaded_func_names(body), &mut out);

        // Extern prototypes for LoadFunc names not defined by the prelude.
        self.emit_externs(body, &mut out, &HashMap::new())?;

        // The body *is* `main`: it prints its result and exits 0.
        self.emit_function_c(
            &mut out,
            "int main(void)",
            body,
            ReturnStyle::Main,
            "return 0;",
            &HashMap::new(),
        )?;

        Ok(out)
    }

    /// Emit one C function — `<header> { <body> }` at translation-unit scope,
    /// indented, with `no_ret` appended when the body has no top-level `Ret`.
    /// Shared by the `main` wrapper (single body) and every `static` definition.
    fn emit_function_c(
        &self,
        out: &mut String,
        header: &str,
        body: &[Instr],
        return_style: ReturnStyle,
        no_ret: &str,
        signatures: &HashMap<String, CSignature>,
    ) -> Result<(), CodegenError> {
        // A defined register that is never an operand is dead and must not be
        // declared (it would warn under -Wall).
        let used = used_registers(body);
        let mut saw_ret = false;
        let mut w = IndentedWriter::new(out, "    ");
        writeln!(w, "{} {{", header).unwrap();
        w.indent();
        for instr in body {
            self.emit_instr(instr, &mut w, &used, &mut saw_ret, return_style, signatures)?;
        }
        if !saw_ret {
            writeln!(w, "{}", no_ret).unwrap();
        }
        w.dedent();
        writeln!(w, "}}").unwrap();
        Ok(())
    }

    fn emit_supporting_type_defs(&self, out: &mut String) {
        // Forward-declare every struct tag so typedefs and struct fields can
        // refer to any struct regardless of emission order.
        if !self.structs.is_empty() {
            writeln!(
                out,
                "// declaration of structs representing record types in the program"
            )
            .unwrap();

            for s in &self.structs {
                writeln!(out, "struct {};", self.struct_name(s.id)).unwrap();
            }
            out.push('\n');
        }

        // Function-pointer typedefs (before struct defs, which may use them).
        if !self.fns.is_empty() {
            writeln!(out, "// Function-pointer typedefs").unwrap();
        }

        for fd in &self.fns {
            writeln!(
                out,
                "typedef {} (*{})({});",
                fd.ret,
                self.fn_name(fd.id),
                if fd.params.is_empty() {
                    "void".to_string()
                } else {
                    fd.params.join(", ")
                }
            )
            .unwrap();
        }
        if !self.fns.is_empty() {
            out.push('\n');
        }
        if !self.structs.is_empty() {
            writeln!(out, "// struct definitions").unwrap();
        }
        for s in &self.structs {
            writeln!(out, "struct {} {{", self.struct_name(s.id)).unwrap();
            for (name, cty) in &s.fields {
                writeln!(out, "    {} {};", cty, name).unwrap();
            }
            if let Some(tail) = &s.closed_from {
                writeln!(out, "    /* row closed from {} at codegen */", tail).unwrap();
            }
            out.push_str("};\n\n");
        }
    }

    fn emit_function_definition(
        &self,
        function: &Function,
        signatures: &HashMap<String, CSignature>,
    ) -> Result<String, CodegenError> {
        let signature = &signatures[&function.name];
        let body = &function.body;
        let mut out = String::new();
        self.emit_externs(body, &mut out, signatures)?;

        let param_regs: Vec<_> = function
            .registers
            .iter()
            .take(signature.params.len())
            .collect();
        if param_regs.len() != signature.params.len() {
            return Err(CodegenError::Unsupported(format!(
                "function {} expects {} parameters but only {} registers exist for parameter binding",
                function.name,
                signature.params.len(),
                param_regs.len()
            )));
        }

        let params = if signature.params.is_empty() {
            "void".to_string()
        } else {
            signature
                .params
                .iter()
                .zip(param_regs.iter())
                .map(|(ty, reg)| format!("{} {}", ty, reg))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut dst_ids: HashSet<RegId> = HashSet::new();
        for_each_instr(body, &mut |instr| {
            if let Some(r) = instr.dst() {
                dst_ids.insert(r.id);
            }
        });

        for reg in &param_regs {
            if dst_ids.contains(&reg.id) {
                return Err(CodegenError::Unsupported(format!(
                    "parameter register {} is reassigned in function {}; reserve the first {} register(s) for parameters only",
                    reg,
                    function.name,
                    signature.params.len()
                )));
            }
        }

        let header = format!("static {} {}({})", signature.ret, signature.name, params);
        let no_ret = format!("return {};", default_return_literal(&signature.ret));
        self.emit_function_c(
            &mut out,
            &header,
            body,
            ReturnStyle::Function,
            &no_ret,
            signatures,
        )?;
        out.push('\n');
        Ok(out)
    }

    fn emit_externs(
        &self,
        body: &[Instr],
        out: &mut String,
        signatures: &HashMap<String, CSignature>,
    ) -> Result<(), CodegenError> {
        // Gather LoadFunc signatures in first-seen order, descending into
        // control-flow sub-blocks so a runtime function loaded inside a branch
        // or loop still gets its prototype.
        let mut ordered: Vec<(&str, &FuncType)> = Vec::new();
        for_each_instr(body, &mut |instr| {
            if let Instr::LoadFunc { name, sig, .. } = instr
                && !ordered.iter().any(|(n, _)| *n == name.as_str())
            {
                ordered.push((name.as_str(), sig));
            }
        });

        if !ordered.is_empty() {
            writeln!(out, "// IR defined functions").unwrap();
        }

        let mut seen: Vec<&str> = Vec::new();
        for (name, sig) in ordered {
            if crate::prelude::get(name).is_some() || signatures.contains_key(name) {
                continue;
            }
            seen.push(name);
            let mut params = Vec::with_capacity(sig.params.len());
            for p in &sig.params {
                // lowering here only reads the (already populated) tables
                params.push(self.lower_readonly(p)?);
            }
            let ret = self.lower_readonly(&sig.ret)?;
            let params = if params.is_empty() {
                "void".to_string()
            } else {
                params.join(", ")
            };
            writeln!(out, "extern {} {}({});", ret, name, params).unwrap();
        }
        if !seen.is_empty() {
            out.push('\n');
        }
        Ok(())
    }

    /// Lower a type against the already-populated interning tables, without
    /// mutating them. Every type reachable from a register signature was
    /// already interned during `ground_registers`, so a miss is a bug.
    fn lower_readonly(&self, ty: &Type) -> Result<String, CodegenError> {
        match ty {
            Type::Int => Ok("int64_t".to_string()),
            Type::Bool => Ok("bool".to_string()),
            Type::Unit => Ok("unit_t".to_string()),
            Type::Ptr(inner) => Ok(format!("{}*", self.lower_readonly(inner)?)),
            Type::Record(row) => {
                let mut fields = Vec::with_capacity(row.fields.len());
                for (name, fty) in &row.fields {
                    fields.push((name.clone(), self.lower_readonly(fty)?));
                }
                match self.struct_index.get(&fields) {
                    Some(id) => Ok(format!("struct {} *", self.struct_name(*id))),
                    None => Err(CodegenError::Unsupported(format!(
                        "record shape {:?} was not interned",
                        fields
                    ))),
                }
            }
            Type::Func(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in &ft.params {
                    params.push(self.lower_readonly(p)?);
                }
                let ret = self.lower_readonly(&ft.ret)?;
                match self.fn_index.get(&(params.clone(), ret.clone())) {
                    Some(id) => Ok(self.fn_name(*id)),
                    None => Err(CodegenError::Unsupported(
                        "function signature was not interned".to_string(),
                    )),
                }
            }
            Type::Unknown(tv) => Err(CodegenError::UnresolvedType(format!("{}", tv))),
            Type::Interface(_) => Err(CodegenError::Unsupported("interface".to_string())),
            Type::Existential(_) => Err(CodegenError::Unsupported("existential".to_string())),
            Type::Stack(_) => Err(CodegenError::Unsupported("stack".to_string())),
        }
    }

    fn emit_instr<W: fmt::Write>(
        &self,
        instr: &Instr,
        out: &mut IndentedWriter<W>,
        used: &std::collections::HashSet<RegId>,
        saw_ret: &mut bool,
        return_style: ReturnStyle,
        signatures: &HashMap<String, CSignature>,
    ) -> Result<(), CodegenError> {
        // Leading comment. Control-flow instructions render multi-line via
        // `Display`, so give them a one-line header instead of dumping the
        // whole block into a `//` comment.
        match instr {
            Instr::If(f) => {
                let _ = writeln!(
                    out,
                    "// if {} -> {}: {}",
                    f.cond,
                    f.dst,
                    self.solver.apply(f.dst.ty())
                );
            }
            Instr::For(f) => {
                let _ = writeln!(
                    out,
                    "// for {} in 0..{}, acc {}: {}",
                    f.index,
                    f.bound,
                    f.acc,
                    self.solver.apply(f.acc.ty())
                );
            }
            _ => {
                if let Some(dst) = instr.dst() {
                    let _ = writeln!(
                        out,
                        "// {} // {}: {}",
                        instr,
                        dst,
                        self.solver.apply(dst.ty())
                    );
                } else {
                    let _ = writeln!(out, "// {}", instr);
                }
            }
        }

        match instr {
            Instr::Const { dst, value } => {
                let lit = match value {
                    crate::instructions::Value::Int(v) => v.to_string(),
                    crate::instructions::Value::Bool(v) => v.to_string(),
                    crate::instructions::Value::Unit => "UNIT".to_string(),
                };
                writeln!(out, "{} {} = {};", self.ctype(dst.id), dst, lit).unwrap();
            }
            Instr::NewObj { dst, fields } => {
                writeln!(
                    out,
                    "{} {} = calloc(1, sizeof *{});",
                    self.ctype(dst.id),
                    dst,
                    dst
                )
                .unwrap();
                for (name, reg) in fields {
                    writeln!(out, "{}->{} = {};", dst, name, reg).unwrap();
                }
            }
            Instr::Load { dst, src, field } => {
                writeln!(out, "{} {} = {}->{};", self.ctype(dst.id), dst, src, field).unwrap();
            }
            Instr::Store { dst, field, src } => {
                writeln!(out, "{}->{} = {};", dst, field, src).unwrap();
            }
            Instr::BinOp { dst, op, lhs, rhs } => {
                writeln!(
                    out,
                    "{} {} = {} {} {};",
                    self.ctype(dst.id),
                    dst,
                    lhs,
                    op.symbol(),
                    rhs
                )
                .unwrap();
            }
            Instr::LoadFunc { dst, name, .. } => {
                // Resolve a call to another in-program function to its mangled
                // C symbol; otherwise the name is an extern and used verbatim.
                let lowered = signatures
                    .get(name)
                    .map(|s| s.name.as_str())
                    .unwrap_or(name);
                writeln!(out, "{} {} = {};", self.ctype(dst.id), dst, lowered).unwrap();
            }
            Instr::Call { func, args, ret } => {
                let args: Vec<String> = args.iter().map(|r| format!("{}", r)).collect();
                if used.contains(&ret.id) {
                    writeln!(
                        out,
                        "{} {} = {}({});",
                        self.ctype(ret.id),
                        ret,
                        func,
                        args.join(", ")
                    )
                    .unwrap();
                } else {
                    // result discarded — emit the call for its side effect only
                    writeln!(out, "{}({});", func, args.join(", ")).unwrap();
                }
            }
            Instr::If(f) => {
                // `dst` is hoisted: declared before the `if`, assigned at the
                // end of whichever branch runs.
                writeln!(out, "{} {};", self.ctype(f.dst.id), f.dst).unwrap();
                writeln!(out, "if ({}) {{", f.cond).unwrap();
                out.indent();
                for i in &f.then_.instructions {
                    self.emit_instr(i, out, used, saw_ret, return_style, signatures)?;
                }
                writeln!(out, "{} = {};", f.dst, f.then_.result).unwrap();
                out.dedent();
                writeln!(out, "}} else {{").unwrap();
                out.indent();
                for i in &f.else_.instructions {
                    self.emit_instr(i, out, used, saw_ret, return_style, signatures)?;
                }
                writeln!(out, "{} = {};", f.dst, f.else_.result).unwrap();
                out.dedent();
                writeln!(out, "}}").unwrap();
            }
            Instr::For(f) => {
                // `acc` is hoisted and seeded from `init`; `index` lives in the
                // loop header; the invariant `acc = body.result` is applied each
                // pass.
                writeln!(out, "{} {} = {};", self.ctype(f.acc.id), f.acc, f.init).unwrap();
                writeln!(
                    out,
                    "for ({} {} = 0; {} < {}; {}++) {{",
                    self.ctype(f.index.id),
                    f.index,
                    f.index,
                    f.bound,
                    f.index
                )
                .unwrap();
                out.indent();
                for i in &f.body.instructions {
                    self.emit_instr(i, out, used, saw_ret, return_style, signatures)?;
                }
                writeln!(
                    out,
                    "{} = {}; // synthesized accumulator write-back",
                    f.acc, f.body.result
                )
                .unwrap();
                out.dedent();
                writeln!(out, "}}").unwrap();
            }
            Instr::Ret { src } => {
                // Depth 1 is the function body; anything deeper is inside a block.
                if out.depth() > 1 {
                    return Err(CodegenError::Unsupported(
                        "ret inside a block is not supported".to_string(),
                    ));
                }
                match return_style {
                    // Entry wrapper: print the result (adapting to its type) and
                    // exit 0.
                    ReturnStyle::Main => {
                        if let Some(r) = src {
                            match self.ctype(r.id) {
                                "int64_t" => {
                                    writeln!(out, "printf(\"result: %lld\\n\", (long long){});", r)
                                        .unwrap();
                                }
                                "bool" => {
                                    writeln!(
                                        out,
                                        "printf(\"result: %s\\n\", {} ? \"true\" : \"false\");",
                                        r
                                    )
                                    .unwrap();
                                }
                                // unit or other: nothing to print; consume the
                                // value so it isn't flagged unused under -Wall.
                                _ => writeln!(out, "(void){};", r).unwrap(),
                            }
                        }
                        writeln!(out, "return 0;").unwrap();
                    }
                    // Ordinary function: return the value. A valueless `ret`
                    // yields unit (the function's return type is `unit_t`).
                    ReturnStyle::Function => match src {
                        Some(r) => writeln!(out, "return {};", r).unwrap(),
                        None => writeln!(out, "return UNIT;").unwrap(),
                    },
                }
                *saw_ret = true;
            }
        }
        Ok(())
    }
}

fn default_return_literal(ret_ctype: &str) -> &'static str {
    if ret_ctype == "bool" {
        "false"
    } else if ret_ctype == "unit_t" {
        "UNIT"
    } else if ret_ctype.ends_with('*') {
        "NULL"
    } else {
        "0"
    }
}

/// Emit a runnable C translation unit for `body`, taking every register's
/// concrete type from `solver`. Records lower to heap-allocated structs behind
/// pointers; unresolved register types are an error rather than a silent
/// default.
pub(crate) fn emit_body(
    body: &[Instr],
    registers: &RegisterFile,
    solver: &Solver,
) -> Result<String, CodegenError> {
    let mut cg = CodeGen::new(solver);
    cg.ground_registers(registers)?;
    cg.emit(body)
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_';
        if ok {
            if i == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Emit a full C translation unit for an IR program with multiple internal
/// function definitions and an explicit entry function.
pub(crate) fn emit_code(program: &Program) -> Result<String, CompilerError> {
    let mut seen_names = HashSet::new();
    for function in &program.functions {
        if !seen_names.insert(function.name.clone()) {
            return Err(CompilerError::DuplicateFunction(function.name.clone()));
        }
    }

    if !seen_names.contains(&program.entry) {
        return Err(CompilerError::MissingEntry(program.entry.clone()));
    }

    let entry_function = program
        .function(&program.entry)
        .expect("entry was validated to exist");
    if !entry_function.signature.params.is_empty() {
        return Err(CompilerError::UnsupportedProgram(format!(
            "entry function {:?} has {} params; main wrapper currently supports zero-arg entry only",
            entry_function.name,
            entry_function.signature.params.len()
        )));
    }

    // Mangle each IR name to a unique C symbol. `main` is reserved for the entry
    // wrapper (`int main(void)`), so a function literally named `main` is bumped.
    let mut used_symbols: HashSet<String> = HashSet::from(["main".to_string()]);
    let mut c_names: HashMap<String, String> = HashMap::new();
    for function in &program.functions {
        let base = sanitize_ident(&function.name);
        let mut candidate = base.clone();
        let mut suffix = 1_u32;
        while used_symbols.contains(&candidate) {
            candidate = format!("{}_{}", base, suffix);
            suffix += 1;
        }
        used_symbols.insert(candidate.clone());
        c_names.insert(function.name.clone(), candidate);
    }

    // First pass: solve each function and lower its signature. `signatures` is
    // the single per-function record that then flows to every emission site
    // (prototypes, definitions, internal-call resolution, the entry wrapper).
    let mut signatures: HashMap<String, CSignature> = HashMap::new();
    for function in &program.functions {
        let mut tvg = type_var_generator_for_function(function);
        let solved = crate::infer::Inference::for_function(function, &mut tvg).solve(&mut tvg)?;
        let mut cg = CodeGen::with_prefix(
            &solved.solver,
            format!("{}_", sanitize_ident(&function.name)),
        );
        cg.ground_registers(&function.registers)?;

        let c_name = c_names[&function.name].clone();
        let sig = cg.lower_signature(c_name, &function.signature)?;
        signatures.insert(function.name.clone(), sig);
    }

    let mut out = String::new();
    out.push_str(PREAMBLE);

    // Internal function prototypes so `LoadFunc` can reference in-program defs
    // regardless of definition order.
    for function in &program.functions {
        let sig = &signatures[&function.name];
        writeln!(
            out,
            "static {} {}({});",
            sig.ret,
            sig.name,
            sig.param_list()
        )
        .unwrap();
    }
    out.push('\n');

    let loaded_all: HashSet<String> = program
        .functions
        .iter()
        .flat_map(|f| loaded_func_names(&f.body).into_iter())
        .collect();
    emit_prelude(&loaded_all, &mut out);

    for function in &program.functions {
        let mut tvg = type_var_generator_for_function(function);
        let solved = crate::infer::Inference::for_function(function, &mut tvg).solve(&mut tvg)?;
        let mut cg = CodeGen::with_prefix(
            &solved.solver,
            format!("{}_", sanitize_ident(&function.name)),
        );
        cg.ground_registers(&function.registers)?;
        cg.emit_supporting_type_defs(&mut out);
        out.push_str(&cg.emit_function_definition(function, &signatures)?);
    }

    let entry = &signatures[&program.entry];
    let entry_ret = &entry.ret;
    writeln!(out, "int main(void) {{").unwrap();
    writeln!(out, "    {} result = {}();", entry_ret, entry.name).unwrap();
    if entry_ret == "int64_t" {
        out.push_str("    printf(\"result: %lld\\n\", result);\n");
    } else if entry_ret == "bool" {
        out.push_str("    printf(\"result: %s\\n\", result ? \"true\" : \"false\");\n");
    }
    out.push_str("    return 0;\n");
    out.push_str("}\n");

    Ok(out)
}

/// Visit every instruction in `body`, descending into the sub-blocks of
/// control-flow instructions.
fn for_each_instr<'a>(body: &'a [Instr], f: &mut impl FnMut(&'a Instr)) {
    for instr in body {
        f(instr);
        match instr {
            Instr::If(i) => {
                for_each_instr(&i.then_.instructions, f);
                for_each_instr(&i.else_.instructions, f);
            }
            Instr::For(i) => for_each_instr(&i.body.instructions, f),
            _ => {}
        }
    }
}

/// Registers that appear as an *operand* of some instruction. A register that
/// is only ever a destination is dead; declaring it would warn under -Wall.
fn used_registers(body: &[Instr]) -> std::collections::HashSet<RegId> {
    let mut used = std::collections::HashSet::new();
    for_each_instr(body, &mut |instr| match instr {
        Instr::Load { src, .. } => {
            used.insert(src.id);
        }
        Instr::Store { dst, src, .. } => {
            used.insert(dst.id);
            used.insert(src.id);
        }
        Instr::NewObj { fields, .. } => {
            for (_, reg) in fields {
                used.insert(reg.id);
            }
        }
        Instr::Call { func, args, .. } => {
            used.insert(func.id);
            for a in args {
                used.insert(a.id);
            }
        }
        Instr::BinOp { lhs, rhs, .. } => {
            used.insert(lhs.id);
            used.insert(rhs.id);
        }
        Instr::Ret { src } => {
            if let Some(r) = src {
                used.insert(r.id);
            }
        }
        Instr::If(f) => {
            used.insert(f.cond.id);
            used.insert(f.then_.result.id);
            used.insert(f.else_.result.id);
        }
        Instr::For(f) => {
            used.insert(f.bound.id);
            used.insert(f.init.id);
            used.insert(f.body.result.id);
        }
        Instr::Const { .. } | Instr::LoadFunc { .. } => {}
    });
    used
}

/// Names of runtime functions the program loads via `LoadFunc`.
fn loaded_func_names(body: &[Instr]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for_each_instr(body, &mut |instr| {
        if let Instr::LoadFunc { name, .. } = instr {
            names.insert(name.clone());
        }
    });
    names
}

/// Append the C definition of every prelude function the program actually
/// loads, in table order. Unloaded prelude functions are skipped so the
/// translation unit stays warning-clean under `-Wall`.
fn emit_prelude(loaded: &HashSet<String>, out: &mut String) {
    let mut any = false;

    writeln!(out, "// prelude functions").unwrap();

    for f in crate::prelude::PRELUDE {
        if loaded.contains(f.name) {
            out.push_str(f.code);
            out.push('\n');
            any = true;
        }
    }
    if any {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::Constraint;
    use crate::instructions::BinOpKind;
    use crate::types::{Row, Type};
    use crate::{CBuild, FunctionBuilder, InstructionBuilder, Program, Solver};
    use std::collections::BTreeMap;

    /// Solve the builder's body and emit C, mirroring the real pipeline.
    fn emit(builder: &mut InstructionBuilder) -> Result<String, CodegenError> {
        emit_with(builder, Vec::new())
    }

    fn emit_with(
        builder: &mut InstructionBuilder,
        extra: Vec<Constraint>,
    ) -> Result<String, CodegenError> {
        let body = builder.body.clone();
        let registers = std::mem::take(&mut builder.register_file);
        crate::infer::Inference::new(&body, &registers)
            .seed(extra)
            .generate_constraints(&mut builder.type_variable_generator)
            .solve(&mut builder.type_variable_generator)
            .expect("constraints should solve")
            .generate_code()
    }

    #[test]
    fn row_extended_field_reaches_struct() {
        // new { x } then store obj.y — the struct must contain both x and y,
        // though y was never part of the NewObj literal.
        let mut b = InstructionBuilder::default();
        let n = b.const_int(1);
        let obj = b.new_obj(vec![("x", n)]);
        let m = b.const_int(2);
        b.store(obj, "y", m);

        let c = emit(&mut b).expect("should emit");

        assert!(c.contains("struct R0 {"), "expected a struct def:\n{}", c);
        // both fields present in the (single) struct
        let struct_body = c
            .split("struct R0 {")
            .nth(1)
            .and_then(|s| s.split("};").next())
            .unwrap();
        assert!(struct_body.contains("x;"), "x missing:\n{}", c);
        assert!(struct_body.contains("y;"), "y missing:\n{}", c);
    }

    #[test]
    fn structural_dedup_shares_one_struct() {
        let mut b = InstructionBuilder::default();
        let a = b.const_int(1);
        let c1 = b.const_int(2);
        let _o1 = b.new_obj(vec![("x", a)]);
        let _o2 = b.new_obj(vec![("x", c1)]);

        let c = emit(&mut b).expect("should emit");

        assert_eq!(
            c.matches("struct R0 {").count(),
            1,
            "identical shapes must share one struct:\n{}",
            c
        );
        assert!(
            !c.contains("struct R1 {"),
            "no second struct expected:\n{}",
            c
        );
    }

    #[test]
    fn loadfunc_signature_drives_inference() {
        // print_int : (int) -> unit applied to a loaded field forces that
        // field (and register) to int.
        let mut b = InstructionBuilder::default();
        let n = b.const_int(7);
        let obj = b.new_obj(vec![("x", n)]);
        let x = b.load(obj, "x");
        let f = b.prelude("print_int");
        let _r = b.call(f, vec![x]);
        b.ret(x);

        let c = emit(&mut b).expect("should emit");

        assert!(
            c.contains("typedef unit_t (*fn0)(int64_t);"),
            "expected fn typedef matching the signature:\n{}",
            c
        );
        // print_int is a prelude function — no extern prototype for it
        assert!(!c.contains("extern"), "prelude fn needs no extern:\n{}", c);
    }

    #[test]
    fn unknown_loadfunc_gets_extern() {
        let mut b = InstructionBuilder::default();
        let n = b.const_int(3);
        let f = b.func("triple", vec![Type::Int], Type::Int);
        let r = b.call(f, vec![n]);
        b.ret(r);

        let c = emit(&mut b).expect("should emit");
        assert!(
            c.contains("extern int64_t triple(int64_t);"),
            "expected extern prototype:\n{}",
            c
        );
    }

    #[test]
    fn unresolved_type_is_an_error() {
        // A register whose type is never constrained cannot be lowered.
        let mut b = InstructionBuilder::default();
        let _dangling = b.reg();

        let err = emit(&mut b).expect_err("unresolved type must error");
        assert!(matches!(err, CodegenError::UnresolvedType(_)));
    }

    #[test]
    fn program_emits_internal_function_without_extern() {
        let mut helper = InstructionBuilder::default();
        let forty = helper.const_int(40);
        let two = helper.const_int(2);
        let sum = helper.binop(BinOpKind::Add, forty, two);
        helper.ret(sum);
        let helper_fn = helper.finish("helper", vec![], Type::Int);

        let mut entry = InstructionBuilder::default();
        let f = entry.func("helper", vec![], Type::Int);
        let result = entry.call(f, vec![]);
        entry.ret(result);
        let entry_fn = entry.finish("main", vec![], Type::Int);

        let mut program = Program::new("main");
        program.add_function(helper_fn);
        program.add_function(entry_fn);

        let c = emit_code(&program).expect("program should emit");

        assert!(c.contains("static int64_t helper(void);"), "{}", c);
        // The entry function is named `main` in the IR; `main` is reserved for
        // the wrapper, so its C symbol is bumped to `main_1`.
        assert!(c.contains("static int64_t main_1(void);"), "{}", c);
        assert!(c.contains("int main(void)"), "{}", c);
        assert!(
            !c.contains("extern int64_t helper(void);"),
            "internal function must not be extern:\n{}",
            c
        );
    }

    #[test]
    fn program_emits_internal_function_with_parameters() {
        let mut helper = FunctionBuilder::new("helper", [Type::Int], Type::Int);
        let arg = helper.param(0);
        let forty = helper.const_int(40);
        let two = helper.const_int(2);
        let partial = helper.binop(BinOpKind::Add, arg, forty);
        let total = helper.binop(BinOpKind::Add, partial, two);
        helper.ret(total);
        let helper_fn = helper.build();

        let mut entry = InstructionBuilder::default();
        let n = entry.const_int(123);
        let f = entry.func("helper", vec![Type::Int], Type::Int);
        let result = entry.call(f, vec![n]);
        entry.ret(result);
        let entry_fn = entry.finish("main", vec![], Type::Int);

        let mut program = Program::new("main");
        program.add_function(helper_fn);
        program.add_function(entry_fn);

        let c = emit_code(&program).expect("program should emit");

        assert!(c.contains("static int64_t helper(int64_t);"), "{}", c);
        assert!(c.contains("static int64_t helper(int64_t reg0)"), "{}", c);
        assert!(c.contains("reg1 = helper;"), "{}", c);
        assert!(c.contains("int64_t reg2 = reg1(reg0);"), "{}", c);
    }

    #[test]
    fn program_requires_existing_entry_function() {
        let mut b = InstructionBuilder::default();
        let n = b.const_int(1);
        b.ret(n);
        let f = b.finish("other", vec![], Type::Int);

        let mut program = Program::new("main");
        program.add_function(f);

        let err = emit_code(&program).expect_err("missing entry must fail");
        assert!(matches!(err, CompilerError::MissingEntry(_)));
    }

    #[test]
    fn unit_returning_functions() {
        // greet(): a side effect, then a valueless `ret` (returns unit).
        let mut greet = InstructionBuilder::default();
        let five = greet.const_int(5);
        let p = greet.prelude("print_int");
        let _ = greet.call(p, vec![five]);
        greet.ret_unit();
        let greet_fn = greet.finish("greet", vec![], Type::Unit);

        // mk_unit(): returns a materialized unit constant.
        let mut mk = InstructionBuilder::default();
        let u = mk.const_unit();
        mk.ret(u);
        let mk_fn = mk.finish("mk_unit", vec![], Type::Unit);

        // main(): call both (results dropped), return 0.
        let mut entry = InstructionBuilder::default();
        let g = entry.func("greet", vec![], Type::Unit);
        let _ = entry.call(g, vec![]);
        let m = entry.func("mk_unit", vec![], Type::Unit);
        let _ = entry.call(m, vec![]);
        let zero = entry.const_int(0);
        entry.ret(zero);
        let entry_fn = entry.finish("main", vec![], Type::Int);

        let mut program = Program::new("main");
        program.add_function(greet_fn);
        program.add_function(mk_fn);
        program.add_function(entry_fn);

        let c = emit_code(&program).expect("program should emit");

        assert!(c.contains("static unit_t greet(void)"), "{}", c);
        assert!(c.contains("static unit_t mk_unit(void)"), "{}", c);
        assert!(c.contains("static const unit_t UNIT = {0};"), "{}", c);
        assert!(c.contains("return UNIT;"), "{}", c);
        assert!(!c.contains("return (unit_t){0};"), "{}", c);
    }

    #[test]
    fn ret_unit_in_non_unit_function_is_rejected() {
        // A valueless `ret` requires a unit return type.
        let mut b = InstructionBuilder::default();
        b.ret_unit();
        let f = b.finish("bad", vec![], Type::Int);

        let mut program = Program::new("bad");
        program.add_function(f);

        let err = emit_code(&program).expect_err("ret_unit in an int fn must fail");
        assert!(matches!(err, CompilerError::Type(_)), "{:?}", err);
    }

    #[test]
    fn unsupported_type_is_an_error() {
        // Force a register to an interface type and confirm it is rejected.
        let mut b = InstructionBuilder::default();
        let r = b.reg();
        let iface = Type::Interface(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int)]),
            tail: None,
        });

        let err = emit_with(&mut b, vec![Constraint::Equal(r.ty(), iface)])
            .expect_err("interface-typed register must error");
        assert!(matches!(err, CodegenError::Unsupported(_)));
    }

    #[test]
    fn binop_lt_yields_bool() {
        let mut b = InstructionBuilder::default();
        let a = b.const_int(1);
        let c1 = b.const_int(2);
        let lt = b.binop(BinOpKind::Lt, a, c1);

        // give the pipeline a valid solve and inspect the declared type
        let body = b.body.clone();
        let registers = std::mem::take(&mut b.register_file);
        let solved = crate::infer::Inference::new(&body, &registers)
            .generate_constraints(&mut b.type_variable_generator)
            .solve(&mut b.type_variable_generator)
            .unwrap();

        assert_eq!(solved.solver.apply(lt.ty()), Type::Bool);
        let c = solved.generate_code().expect("should emit");
        assert!(
            c.contains(&format!("bool {} =", lt)),
            "lt should be bool:\n{}",
            c
        );
    }

    #[test]
    fn if_merges_branch_results() {
        // if true { 1 } else { 2 } — dst is a hoisted int assigned in both arms.
        let mut b = InstructionBuilder::default();
        let cond = b.const_bool(true);
        let dst = b.if_value(cond, |b| b.const_int(1), |b| b.const_int(2));
        b.ret(dst);

        let c = emit(&mut b).expect("should emit");

        assert!(
            c.contains(&format!("int64_t {};", dst)),
            "expected hoisted merge decl:\n{}",
            c
        );
        assert!(
            c.contains("if (") && c.contains("} else {"),
            "expected if/else:\n{}",
            c
        );
        // dst assigned once per branch
        assert_eq!(
            c.matches(&format!("{} = ", dst)).count(),
            2,
            "dst must be assigned in both branches:\n{}",
            c
        );
    }

    #[test]
    fn for_accumulator() {
        // sum = for i in 0..10, acc = 0 { acc + i }
        let mut b = InstructionBuilder::default();
        let ten = b.const_int(10);
        let zero = b.const_int(0);
        let sum = b.for_acc(ten, zero, |b, index, acc| {
            b.binop(BinOpKind::Add, acc, index)
        });
        b.ret(sum);

        let c = emit(&mut b).expect("should emit");

        // acc is hoisted and seeded from init (both int64_t)
        assert!(
            c.contains(&format!("int64_t {} = ", sum)),
            "expected hoisted accumulator init:\n{}",
            c
        );
        assert!(c.contains("for (int64_t"), "expected a C for loop:\n{}", c);
        // the loop epilogue writes the next value back into the accumulator
        assert!(
            c.contains(&format!("{} = ", sum)),
            "expected accumulator update:\n{}",
            c
        );
    }

    #[test]
    fn for_invariant_mismatch_rejected() {
        // acc is seeded from an int but the body yields a bool — the checked
        // invariant Equal(acc, next) must make the solve fail.
        let mut b = InstructionBuilder::default();
        let ten = b.const_int(10);
        let zero = b.const_int(0);
        let _sum = b.for_acc(ten, zero, |b, _i, _acc| b.const_bool(true));

        let body = b.body.clone();
        let constraints = crate::infer::constraints_for(&body, &mut b.type_variable_generator);
        let mut solver = Solver::new(&mut b.type_variable_generator);
        assert!(
            solver.solve(&constraints).is_err(),
            "mismatched loop invariant (int acc vs bool body) must fail to type-check"
        );
    }

    #[test]
    fn loadfunc_inside_block_gets_extern() {
        // A runtime function loaded inside a loop body must still get its
        // prototype — guards the recursive walkers.
        let mut b = InstructionBuilder::default();
        let ten = b.const_int(10);
        let zero = b.const_int(0);
        let sum = b.for_acc(ten, zero, |b, index, acc| {
            let f = b.func("sink", vec![Type::Int], Type::Int);
            let _ = b.call(f, vec![index]);
            b.binop(BinOpKind::Add, acc, index)
        });
        b.ret(sum);

        let c = emit(&mut b).expect("should emit");
        assert!(
            c.contains("extern int64_t sink(int64_t);"),
            "extern for a func loaded inside a block is missing:\n{}",
            c
        );
    }

    #[test]
    fn ret_inside_block_is_unsupported() {
        let mut b = InstructionBuilder::default();
        let cond = b.const_bool(true);
        let dst = b.if_value(
            cond,
            |b| {
                let one = b.const_int(1);
                b.ret(one);
                one
            },
            |b| b.const_int(2),
        );
        b.ret(dst);

        let err = emit(&mut b).expect_err("ret inside a block must be rejected");
        assert!(matches!(err, CodegenError::Unsupported(_)));
    }

    #[test]
    fn if_generates_cond_and_merge_constraints() {
        // Equal(cond, Bool) + one Equal per branch const + two merge Equals.
        let mut b = InstructionBuilder::default();
        let cond = b.const_bool(true);
        let _dst = b.if_value(cond, |b| b.const_int(1), |b| b.const_int(2));
        let if_instr = b
            .body
            .iter()
            .find(|i| matches!(i, Instr::If(_)))
            .expect("If instruction was built")
            .clone();

        let cs = crate::infer::constraints_from_instr(&if_instr, &mut b.type_variable_generator);
        assert_eq!(cs.len(), 5, "unexpected If constraint shape: {:?}", cs);
    }

    #[test]
    #[ignore = "requires a C compiler; run manually in verification"]
    fn compile_and_run_smoke() {
        let mut b = InstructionBuilder::default();
        let n = b.const_int(42);
        let obj = b.new_obj(vec![("x", n)]);
        b.store(obj, "y", n);
        let y = b.load(obj, "y");
        let one = b.const_int(1);
        let sum = b.binop(BinOpKind::Add, y, one);
        let f = b.prelude("print_int");
        let _ = b.call(f, vec![sum]);
        b.store(obj, "z", sum);
        b.ret(sum);

        // compile and run the code above into a temp file
        let report = CBuild::from_builder("italics_smoke", b)
            .expect("should build")
            .dir(std::env::temp_dir().join("italics_smoke_build"))
            .generate()
            .expect("generate")
            .compile()
            .expect("compile")
            .run()
            .expect("run should succeed");
        let stdout = report.stdout;
        assert!(stdout.contains("43"), "expected 43, got: {}", stdout);
    }
}
