use std::fmt;
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    FunctionArrow,
    RowMembershipOperator,
    SubtypeOperator,
    SubstitutionArrow,
    TypeVarLetter,
    RowVarLetter,
}

// ->  - U+002D U+003E
// :in:
// <:
// =>
// t / r
const ASCII_SYMBOLS: [&'static str; 6] = ["->", ":in:", "<:", "=>", "t", "r"];

const UNICODE_SYMBOLS: [&'static str; 6] = ["→", "∈", "⊆", "↦", "τ", "ρ"];

const fn symbol_index(symbol: Symbol) -> usize {
    match symbol {
        Symbol::FunctionArrow => 0,
        Symbol::RowMembershipOperator => 1,
        Symbol::SubtypeOperator => 2,
        Symbol::SubstitutionArrow => 3,
        Symbol::TypeVarLetter => 4,
        Symbol::RowVarLetter => 5,
    }
}

const fn symbol_table() -> &'static [&'static str; 6] {
    if cfg!(feature = "pretty-unicode") {
        &UNICODE_SYMBOLS
    } else {
        &ASCII_SYMBOLS
    }
}

pub fn symbol(kind: Symbol) -> &'static str {
    let table = symbol_table();
    table[symbol_index(kind)]
}

pub fn type_var_letter(is_row: bool) -> char {
    let text = if is_row {
        symbol(Symbol::RowVarLetter)
    } else {
        symbol(Symbol::TypeVarLetter)
    };

    text.chars()
        .next()
        .expect("type variable symbol table entries must be non-empty")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Subscript(pub u32);

impl fmt::Display for Subscript {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !cfg!(feature = "pretty-unicode") {
            return write!(fmt, "_{}", self.0);
        }

        let mut n = self.0;

        // Special case for zero.
        if n == 0 {
            return fmt.write_char('\u{2080}');
        }

        // Maximum digits for u32 is 10 (4_294_967_295).
        let mut buf = [0u8; 10];

        // Extract digits backwards.
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

#[cfg(test)]
mod tests {
    use crate::display::{Symbol, symbol};
    use crate::variables::TypeVar;

    #[test]
    fn type_var_display_matches_selected_style() {
        let ty = TypeVar(12);
        let row = TypeVar(0x8000_0005);

        if cfg!(feature = "pretty-unicode") {
            assert_eq!(format!("{}", ty), "τ₁₂");
            assert_eq!(format!("{}", row), "ρ₅");
            assert_eq!(symbol(Symbol::FunctionArrow), "→");
            assert_eq!(symbol(Symbol::RowMembershipOperator), "∈");
            assert_eq!(symbol(Symbol::SubtypeOperator), "⊆");
            assert_eq!(symbol(Symbol::SubstitutionArrow), "↦");
        } else {
            assert_eq!(format!("{}", ty), "t_12");
            assert_eq!(format!("{}", row), "r_5");
            assert_eq!(symbol(Symbol::FunctionArrow), "->");
            assert_eq!(symbol(Symbol::RowMembershipOperator), ":in:");
            assert_eq!(symbol(Symbol::SubtypeOperator), "<:");
            assert_eq!(symbol(Symbol::SubstitutionArrow), "=>");
        }
    }
}
