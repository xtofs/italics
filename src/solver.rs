use std::collections::{BTreeMap, HashMap};

use crate::constraints::Constraint;
use crate::ids::TypeVar;
use crate::types::{FuncType, Row, Type};
use crate::{Instr, TypeVarGenerator};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Kind {
    Type,
    Row,
}

#[derive(Debug)]
pub struct Solver<'a> {
    pub subs: HashMap<TypeVar, Type>,
    pub kinds: HashMap<TypeVar, Kind>,
    pub tvg: &'a mut TypeVarGenerator,
}

#[derive(Debug)]
pub enum TypeError {
    UnificationFailed(String),
}

impl<'a> Solver<'a> {
    pub fn new(tvg: &'a mut TypeVarGenerator) -> Self {
        Self {
            subs: HashMap::new(),
            kinds: HashMap::new(),
            tvg,
        }
    }

    pub fn resolve(&self, var: TypeVar) -> Type {
        self.subs.get(&var).cloned().unwrap_or(Type::Unknown(var))
    }

    pub fn unify(&mut self, a: Type, b: Type) -> Result<(), TypeError> {
        let a = self.resolve_type(a);
        let b = self.resolve_type(b);

        match (a, b) {
            (x, y) if x == y => Ok(()),
            (Type::Unknown(tv), ty) => self.bind_var(tv, ty),
            (ty, Type::Unknown(tv)) => self.bind_var(tv, ty),
            (Type::Int, Type::Int) => Ok(()),
            (Type::Bool, Type::Bool) => Ok(()),
            (Type::Ptr(a), Type::Ptr(b)) => self.unify(*a, *b),
            (Type::Func(f1), Type::Func(f2)) => self.unify_func(f1, f2),
            (Type::Record(r1), Type::Record(r2)) => self.unify_row(r1, r2),
            (Type::Interface(r1), Type::Interface(r2)) => self.unify_row(r1, r2),

            (Type::Record(_), Type::Interface(_)) | (Type::Interface(_), Type::Record(_)) => {
                Err(TypeError::UnificationFailed(
                    "record/interface mismatch; use Subtype constraint".into(),
                ))
            }

            (Type::Stack(s1), Type::Stack(s2)) => self.unify_stack(s1, s2),

            (Type::Existential(_), Type::Existential(_)) => Err(TypeError::UnificationFailed(
                "existential unification not implemented yet".into(),
            )),

            (x, y) => Err(TypeError::UnificationFailed(format!(
                "cannot unify {:?} with {:?}",
                x, y
            ))),
        }
    }

    fn bind_var(&mut self, tv: TypeVar, ty: Type) -> Result<(), TypeError> {
        if self.occurs_in(tv, &ty) {
            return Err(TypeError::UnificationFailed(format!(
                "occurs check failed: {:?} in {:?}",
                tv, ty
            )));
        }

        self.subs.insert(tv, ty);
        Ok(())
    }

    fn occurs_in(&self, tv: TypeVar, ty: &Type) -> bool {
        match ty {
            Type::Unknown(v) => *v == tv,
            Type::Ptr(inner) => self.occurs_in(tv, inner),
            Type::Func(f) => {
                f.params.iter().any(|t| self.occurs_in(tv, t))
                    || self.occurs_in(tv, &f.ret)
                    || f.stack
                        .as_ref()
                        .is_some_and(|s| s.iter().any(|t| self.occurs_in(tv, t)))
            }
            Type::Record(row) | Type::Interface(row) => {
                row.fields.values().any(|t| self.occurs_in(tv, t))
                    || row.tail.is_some_and(|tail_tv| tail_tv == tv)
            }
            Type::Stack(ts) => ts.iter().any(|t| self.occurs_in(tv, t)),
            Type::Existential(e) => e.var == tv || self.occurs_in(tv, &e.ty),
            Type::Int | Type::Bool => false,
        }
    }

    fn unify_func(&mut self, f1: FuncType, f2: FuncType) -> Result<(), TypeError> {
        if f1.params.len() != f2.params.len() {
            return Err(TypeError::UnificationFailed(
                "function arity mismatch".into(),
            ));
        }

        for (p1, p2) in f1.params.into_iter().zip(f2.params.into_iter()) {
            self.unify(p1, p2)?;
        }

        self.unify(*f1.ret, *f2.ret)?;

        match (f1.stack, f2.stack) {
            (Some(s1), Some(s2)) => self.unify_stack(s1, s2),
            (None, None) => Ok(()),
            _ => Err(TypeError::UnificationFailed(
                "stack typing mismatch in function types".into(),
            )),
        }
    }

    fn unify_stack(&mut self, s1: Vec<Type>, s2: Vec<Type>) -> Result<(), TypeError> {
        if s1.len() != s2.len() {
            return Err(TypeError::UnificationFailed("stack length mismatch".into()));
        }
        for (t1, t2) in s1.into_iter().zip(s2.into_iter()) {
            self.unify(t1, t2)?;
        }
        Ok(())
    }

    pub fn unify_row(&mut self, r1: Row, r2: Row) -> Result<(), TypeError> {
        if r1.fields.len() != r2.fields.len() {
            return Err(TypeError::UnificationFailed(
                "row field count mismatch".into(),
            ));
        }

        if r1.tail.is_some() || r2.tail.is_some() {
            return Err(TypeError::UnificationFailed(
                "row tails not supported yet in unify_row".into(),
            ));
        }

        for (name, t1) in r1.fields {
            match r2.fields.get(&name) {
                Some(t2) => self.unify(t1.clone(), t2.clone())?,
                None => {
                    return Err(TypeError::UnificationFailed(format!(
                        "row missing field {:?}",
                        name
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn solve(&mut self, constraints: &[Constraint]) -> Result<(), TypeError> {
        for c in constraints {
            match c {
                Constraint::Equal(a, b) => self.unify(a.clone(), b.clone())?,
                Constraint::RowHasField(row_ty, name) => {
                    self.check_row_has_field(row_ty.clone(), name.clone())?;
                }
                Constraint::RowFieldType(row_ty, name, field_ty) => {
                    self.check_row_field_type(row_ty.clone(), name.clone(), field_ty.clone())?;
                }
                Constraint::Subtype(a, b) => match (a, b) {
                    (Type::Record(record_row), Type::Interface(interface_row)) => {
                        self.subtype_record_interface(record_row.clone(), interface_row.clone())?
                    }
                    _ => {
                        return Err(TypeError::UnificationFailed(
                            "Subtype only supported for Record <: Interface".into(),
                        ));
                    }
                },
                Constraint::StackEqual(s1, s2) => {
                    self.unify(Type::Stack(s1.clone()), Type::Stack(s2.clone()))?;
                }
            }
        }
        Ok(())
    }

    fn check_row_has_field(&mut self, row_ty: Type, name: String) -> Result<(), TypeError> {
        match self.resolve_type(row_ty) {
            Type::Record(row) | Type::Interface(row) => {
                if row.fields.contains_key(&name) {
                    return Ok(());
                }

                // Missing field — try row-tail extension
                match row.tail {
                    Some(tail_tv) => {
                        // Create a fresh type variable for the missing field
                        let new_field_tv = self.fresh_tv(Kind::Type);

                        // Create a fresh tail for the extended row fragment
                        let new_tail_tv = self.fresh_tv(Kind::Row);

                        // Build the new row fragment that will replace the old tail
                        let mut new_tail_row = Row {
                            fields: BTreeMap::new(),
                            tail: Some(new_tail_tv),
                        };

                        // Insert the missing field with its fresh type variable
                        new_tail_row
                            .fields
                            .insert(name.clone(), Type::Unknown(new_field_tv));

                        // Bind the old tail variable to this new row fragment
                        self.bind_var(tail_tv, Type::Record(new_tail_row))?;

                        Ok(())
                    }

                    None => Err(TypeError::UnificationFailed(format!(
                        "row missing field {:?} and row is closed",
                        name
                    ))),
                }
            }

            other => Err(TypeError::UnificationFailed(format!(
                "RowHasField on non-row type {:?}",
                other
            ))),
        }
    }

    fn fresh_tv(&mut self, kind: Kind) -> TypeVar {
        let new_tv = self.tvg.fresh();
        self.kinds.insert(new_tv, kind);
        new_tv
    }

    // fn check_row_has_field_x(&mut self, row_ty: Type, name: String) -> Result<(), TypeError> {
    //     match self.resolve_type(row_ty) {
    //         Type::Record(row) | Type::Interface(row) => {
    //             if row.fields.contains_key(&name) {
    //                 Ok(())
    //             } else {
    //                 Err(TypeError::UnificationFailed(format!(
    //                     "row missing field {:?}",
    //                     name
    //                 )))
    //             }
    //         }
    //         other => Err(TypeError::UnificationFailed(format!(
    //             "RowHasField on non-row type {:?}",
    //             other
    //         ))),
    //     }
    // }

    pub fn resolve_type(&self, ty: Type) -> Type {
        match ty {
            Type::Unknown(tv) => self.subs.get(&tv).cloned().unwrap_or(Type::Unknown(tv)),
            _ => ty,
        }
    }

    fn check_row_field_type(
        &mut self,
        row_ty: Type,
        name: String,
        field_ty: Type,
    ) -> Result<(), TypeError> {
        match self.resolve_type(row_ty) {
            Type::Record(row) | Type::Interface(row) => match row.fields.get(&name) {
                Some(t) => self.unify(t.clone(), field_ty),
                None => Err(TypeError::UnificationFailed(format!(
                    "row missing field {:?} for RowFieldType",
                    name
                ))),
            },
            other => Err(TypeError::UnificationFailed(format!(
                "RowFieldType on non-row type {:?}",
                other
            ))),
        }
    }

    fn subtype_record_interface(&mut self, record: Row, interface: Row) -> Result<(), TypeError> {
        for (name, interface_ty) in interface.fields {
            match record.fields.get(&name) {
                Some(obj_ty) => self.unify(obj_ty.clone(), interface_ty.clone())?,
                None => {
                    return Err(TypeError::UnificationFailed(format!(
                        "object missing field {:?} required by interface",
                        name
                    )));
                }
            }
        }

        match (record.tail, interface.tail) {
            (Some(obj_tail), Some(iface_tail)) => {
                self.unify(Type::Unknown(obj_tail), Type::Unknown(iface_tail))
            }
            (Some(_), None) => Ok(()),
            (None, Some(_)) => Err(TypeError::UnificationFailed(
                "interface expects open row but object is closed".into(),
            )),
            (None, None) => Ok(()),
        }
    }

    pub fn generate_constraints(&mut self, instr: &Instr) -> Vec<Constraint> {
        match instr {
            Instr::Load { dst, src, field } => vec![
                Constraint::RowHasField(src.ty(), field.clone()),
                Constraint::RowFieldType(src.ty(), field.clone(), dst.ty()),
            ],

            Instr::Store { dst, field, src } => vec![
                Constraint::RowHasField(dst.ty(), field.clone()),
                Constraint::RowFieldType(dst.ty(), field.clone(), src.ty()),
            ],

            Instr::NewObj { dst, fields } => {
                // Create a fresh row tail type variable
                let tail_tv = self.fresh_tv(Kind::Row);

                let mut row = Row {
                    fields: BTreeMap::new(),
                    tail: Some(tail_tv),
                };

                for (name, reg) in fields {
                    row.fields.insert(name.clone(), reg.ty());
                }

                vec![Constraint::Equal(dst.ty(), Type::Record(row))]
            }

            Instr::Call { func, args, ret } => {
                let func_ty = Type::Func(FuncType {
                    params: args.iter().map(|r| r.ty()).collect(),
                    ret: Box::new(ret.ty()),
                    stack: None,
                });

                vec![Constraint::Equal(func.ty(), func_ty)]
            }
        }
    }
}
