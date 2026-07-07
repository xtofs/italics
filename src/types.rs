use std::collections::BTreeMap;
use std::fmt;

use crate::display::{self, Subscript, Symbol, symbol};
use crate::variables::TypeVar;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
    pub stack: Option<Vec<Type>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Existential {
    pub var: TypeVar,
    pub ty: Box<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Row {
    pub fields: BTreeMap<String, Type>,
    pub tail: Option<TypeVar>,
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ ")?;
        for (i, item) in self.fields.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", item.0, item.1)?;
        }
        if let Some(tail) = self.tail {
            if !self.fields.is_empty() {
                write!(f, " | ")?;
            }
            write!(f, "{}", tail)?;
        }
        write!(f, " }}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Bool,
    /// The singleton type with exactly one value. Lowered to a one-byte C
    /// struct (`unit_t`); used as the return type of side-effecting functions.
    Unit,
    Ptr(Box<Type>),
    Func(FuncType),
    Record(Row),
    Interface(Row),
    Existential(Existential),
    Stack(Vec<Type>),
    Unknown(TypeVar),
}

impl fmt::Display for Type {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(fmt, "int"),
            Type::Bool => write!(fmt, "bool"),
            Type::Unit => write!(fmt, "unit"),
            Type::Ptr(t) => write!(fmt, "ptr({})", t),
            Type::Func(func_type) => {
                write!(fmt, "(")?;
                for (i, item) in func_type.params.iter().enumerate() {
                    if i > 0 {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{item}")?;
                }
                write!(fmt, ") {} {}", symbol(Symbol::FunctionArrow), func_type.ret)
            }
            Type::Record(row) => write!(fmt, "record {}", row),
            Type::Interface(row) => write!(fmt, "interface {}", row),
            Type::Existential(_) => write!(fmt, "Existential"),
            Type::Stack(_) => write!(fmt, "Stack"),
            Type::Unknown(type_var) => write!(fmt, "{}", type_var),
        }
    }
}

impl fmt::Display for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = display::type_var_letter(self.is_row());
        write!(f, "{}{}", letter, Subscript(self.index()))
    }
}
