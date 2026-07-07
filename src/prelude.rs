//! The runtime prelude: side-effecting functions the C backend defines itself.
//!
//! Each entry is the single source of truth for a prelude function — its name,
//! its inferred signature (so the builder can pull it in without the caller
//! re-typing it), and its C definition (so codegen can emit it). Prelude
//! functions return [`Type::Unit`], matching their side-effecting nature.

use crate::types::{FuncType, Type};

/// A runtime function the emitted C prelude defines itself.
pub struct PreludeFn {
    pub name: &'static str,
    pub params: &'static [Type],
    pub ret: Type,
    /// The complete `static … { … }` C definition.
    pub code: &'static str,
}

impl PreludeFn {
    /// The function's type signature, for feeding into inference.
    pub fn signature(&self) -> FuncType {
        FuncType {
            params: self.params.to_vec(),
            ret: Box::new(self.ret.clone()),
            stack: None,
        }
    }
}

/// Every available prelude function.
pub static PRELUDE: &[PreludeFn] = &[
    PreludeFn {
        name: "print_int",
        params: &[Type::Int],
        ret: Type::Unit,
        code: "static unit_t print_int(int64_t x) { printf(\"%lld\\n\", (long long)x); return UNIT; }",
    },
    PreludeFn {
        name: "print_bool",
        params: &[Type::Bool],
        ret: Type::Unit,
        code: "static unit_t print_bool(bool x) { printf(\"%s\\n\", x ? \"true\" : \"false\"); return UNIT; }",
    },
];

/// Look up a prelude function by name.
pub fn get(name: &str) -> Option<&'static PreludeFn> {
    PRELUDE.iter().find(|f| f.name == name)
}
