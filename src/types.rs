use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;

use crate::vars::TypeVar;

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
            Type::Ptr(t) => write!(fmt, "ptr({})", t),
            Type::Func(func_type) => {
                write!(fmt, "(")?;
                for (i, item) in func_type.params.iter().enumerate() {
                    if i > 0 {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{item}")?;
                }
                write!(fmt, ") → {}", func_type.ret)
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
        let letter = if self.is_row() { 'ρ' } else { 'τ' };
        write!(f, "{}{}", letter, Subscript(self.index()))
    }
}

struct Subscript(u32);

impl fmt::Display for Subscript {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut n = self.0;

        // Special case for zero
        if n == 0 {
            return fmt.write_char('\u{2080}');
        }

        // Maximum digits for u32 is 10 (4_294_967_295)
        let mut buf = [0u8; 10];

        // Extract digits backwards
        let mut i = buf.len();
        while n != 0 {
            i -= 1;
            buf[i] = (n % 10) as u8;
            n /= 10;
        }

        for &digit in &buf[i..] {
            let ch = char::from_u32(0x2080 + (digit as u32)).expect("digit should be in 0..=9");
            fmt.write_char(ch)?;
        }

        Ok(())
    }
}
