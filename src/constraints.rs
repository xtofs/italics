use std::fmt;

use crate::display::{Symbol, symbol};
use crate::types::Type;

#[derive(Debug, Clone)]
pub enum Constraint {
    /// Two types must unify.
    Equal(Type, Type),

    /// The row type must **have** the named field (presence only). The field's
    /// type is left unconstrained; on an open row a missing field is added by
    /// row-tail extension, on a closed row a missing field is an error.
    /// Printed `f :in: r` by default (`f ∈ r` with `pretty-unicode`).
    RowHasField(Type, String),

    /// The type of the named field must unify with the given type. This does
    /// **not** require the field to exist — establishing presence is the job
    /// of `RowHasField`, with which this constraint is paired for field
    /// access. Printed `f: t :in: r` by default (`f: t ∈ r` with
    /// `pretty-unicode`).
    RowFieldType(Type, String, Type),

    /// Structural inclusion: a `Record` must satisfy an `Interface`.
    Subtype(Type, Type),

    /// Two stack shapes must unify element-wise.
    StackEqual(Vec<Type>, Vec<Type>),
}

impl fmt::Display for Constraint {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constraint::Equal(a, b) => write!(fmt, "{} = {}", a, b),
            Constraint::RowHasField(r, f) => {
                write!(fmt, "{} {} {}", f, symbol(Symbol::RowMembershipOperator), r)
            }
            Constraint::RowFieldType(r, f, t) => {
                write!(
                    fmt,
                    "{}: {} {} {}",
                    f,
                    t,
                    symbol(Symbol::RowMembershipOperator),
                    r
                )
            }
            Constraint::Subtype(a, b) => {
                write!(fmt, "{} {} {}", a, symbol(Symbol::SubtypeOperator), b)
            }
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
