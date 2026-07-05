use std::fmt;

use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Constraint {
    Equal(Type, Type),
    RowHasField(Type, String),
    RowFieldType(Type, String, Type),
    Subtype(Type, Type),
    StackEqual(Vec<Type>, Vec<Type>),
}

impl fmt::Display for Constraint {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Equal(a, b) => write!(fmt, "{} = {}", a, b),
            Constraint::RowHasField(r, f) => write!(fmt, "{} ∈ {}", f, r),
            Constraint::RowFieldType(r, f, t) => write!(fmt, "{}: {} ∈ {}", f, t, r),
            Constraint::Subtype(a, b) => write!(fmt, "{} ⊆ {}", a, b),
            Constraint::StackEqual(xs, ys) => {
                write!(fmt, " ]")?;
                for (i, (x, y)) in xs.iter().zip(ys).enumerate() {
                    if i > 0 {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{} = {}", x, y)?;
                }
                write!(fmt, " ]")
            }
        }
    }
}
