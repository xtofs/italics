use std::collections::{BTreeMap, HashMap};

use crate::constraints::Constraint;
use crate::instructions::{BinOpKind, Value};
use crate::types::{Existential, FuncType, Row, Type};
use crate::variables::TypeVar;
use crate::{Instr, TypeVarGenerator};

#[derive(Debug)]
pub struct Solver<'a> {
    pub substitutions: HashMap<TypeVar, Type>,
    pub tvg: &'a mut TypeVarGenerator,
}

#[derive(Debug)]
pub enum TypeError {
    UnificationFailed(String),
    KindMismatch(String),
}

impl<'a> Solver<'a> {
    pub fn new(tvg: &'a mut TypeVarGenerator) -> Self {
        Self {
            substitutions: HashMap::new(),
            tvg,
        }
    }

    pub fn resolve(&self, var: TypeVar) -> Type {
        self.substitutions
            .get(&var)
            .cloned()
            .unwrap_or(Type::Unknown(var))
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
            (Type::Unit, Type::Unit) => Ok(()),
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
        self.check_kind(tv, &ty)?;

        if self.occurs_in(tv, &ty) {
            return Err(TypeError::UnificationFailed(format!(
                "occurs check failed: {:?} in {:?}",
                tv, ty
            )));
        }

        self.substitutions.insert(tv, ty);
        Ok(())
    }

    /// A row-kind variable (row tail) may only be bound to a row fragment
    /// (represented as `Record`) or another row-kind variable. A type-kind
    /// variable must never be bound to a row-kind variable.
    fn check_kind(&self, tv: TypeVar, ty: &Type) -> Result<(), TypeError> {
        if tv.is_row() {
            match ty {
                Type::Record(_) => Ok(()),
                Type::Unknown(v) if v.is_row() => Ok(()),
                other => Err(TypeError::KindMismatch(format!(
                    "row variable {} cannot be bound to non-row type {:?}",
                    tv, other
                ))),
            }
        } else {
            match ty {
                Type::Unknown(v) if v.is_row() => Err(TypeError::KindMismatch(format!(
                    "type variable {} cannot be bound to row variable {}",
                    tv, v
                ))),
                _ => Ok(()),
            }
        }
    }

    fn occurs_in(&self, tv: TypeVar, ty: &Type) -> bool {
        match ty {
            // chase substitutions so indirect cycles are caught too
            Type::Unknown(v) => {
                *v == tv
                    || self
                        .substitutions
                        .get(v)
                        .is_some_and(|bound| self.occurs_in(tv, bound))
            }
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
                    || row
                        .tail
                        .is_some_and(|tail_tv| self.occurs_in(tv, &Type::Unknown(tail_tv)))
            }
            Type::Stack(ts) => ts.iter().any(|t| self.occurs_in(tv, t)),
            Type::Existential(e) => e.var == tv || self.occurs_in(tv, &e.ty),
            Type::Int | Type::Bool | Type::Unit => false,
        }
    }

    fn unify_func(&mut self, f1: FuncType, f2: FuncType) -> Result<(), TypeError> {
        if f1.params.len() != f2.params.len() {
            return Err(TypeError::UnificationFailed(
                "function arity mismatch".into(),
            ));
        }

        for (p1, p2) in f1.params.into_iter().zip(f2.params) {
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
        for (t1, t2) in s1.into_iter().zip(s2) {
            self.unify(t1, t2)?;
        }
        Ok(())
    }

    /// Rémy-style row unification: shared fields unify pointwise; fields
    /// exclusive to one side are absorbed into the other side's tail.
    pub fn unify_row(&mut self, r1: Row, r2: Row) -> Result<(), TypeError> {
        let r1 = self.resolve_row(&r1);
        let r2 = self.resolve_row(&r2);

        for (name, t1) in &r1.fields {
            if let Some(t2) = r2.fields.get(name) {
                self.unify(t1.clone(), t2.clone())?;
            }
        }

        let only_in = |a: &Row, b: &Row| -> BTreeMap<String, Type> {
            a.fields
                .iter()
                .filter(|(name, _)| !b.fields.contains_key(*name))
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect()
        };
        let only1 = only_in(&r1, &r2);
        let only2 = only_in(&r2, &r1);

        match (r1.tail, r2.tail) {
            // identical tails cannot absorb differing fields
            (Some(t1), Some(t2)) if t1 == t2 => {
                if only1.is_empty() && only2.is_empty() {
                    Ok(())
                } else {
                    Err(TypeError::UnificationFailed(format!(
                        "rows with the same tail {} differ in fields {:?} vs {:?}",
                        t1,
                        only1.keys().collect::<Vec<_>>(),
                        only2.keys().collect::<Vec<_>>()
                    )))
                }
            }

            (None, None) => {
                if only1.is_empty() && only2.is_empty() {
                    Ok(())
                } else {
                    Err(TypeError::UnificationFailed(format!(
                        "closed rows differ in fields {:?} vs {:?}",
                        only1.keys().collect::<Vec<_>>(),
                        only2.keys().collect::<Vec<_>>()
                    )))
                }
            }

            (Some(t1), None) => {
                if !only1.is_empty() {
                    return Err(TypeError::UnificationFailed(format!(
                        "closed row is missing fields {:?}",
                        only1.keys().collect::<Vec<_>>()
                    )));
                }
                // absorb r2's extra fields and close the row
                self.bind_var(
                    t1,
                    Type::Record(Row {
                        fields: only2,
                        tail: None,
                    }),
                )
            }

            (None, Some(t2)) => {
                if !only2.is_empty() {
                    return Err(TypeError::UnificationFailed(format!(
                        "closed row is missing fields {:?}",
                        only2.keys().collect::<Vec<_>>()
                    )));
                }
                self.bind_var(
                    t2,
                    Type::Record(Row {
                        fields: only1,
                        tail: None,
                    }),
                )
            }

            (Some(t1), Some(t2)) => {
                if only1.is_empty() && only2.is_empty() {
                    return self.bind_var(t1, Type::Unknown(t2));
                }
                // each tail absorbs the other side's exclusive fields,
                // both remaining open through a common fresh tail
                let common = self.tvg.fresh_row();
                self.bind_var(
                    t1,
                    Type::Record(Row {
                        fields: only2,
                        tail: Some(common),
                    }),
                )?;
                self.bind_var(
                    t2,
                    Type::Record(Row {
                        fields: only1,
                        tail: Some(common),
                    }),
                )
            }
        }
    }

    /// Solve priority for a constraint. Constraints are processed in ascending
    /// weight, so the weight encodes the dependencies between constraint kinds
    /// and the order of constraints *within* the input list no longer matters
    /// (beyond ties, which a stable sort preserves):
    ///
    /// 0. **presence** (`RowHasField`) — create/extend every required field
    /// 1. **field types** (`RowFieldType`) — link an existing field's type;
    ///    these never create fields, so all presence must settle first
    /// 2. **definitional** (`Equal`) — bind object/function variables to their
    ///    record/function structure
    /// 3. **relational** (`Subtype` / `StackEqual`) — checks that need every
    ///    referenced structure already defined
    fn weight(c: &Constraint) -> u8 {
        match c {
            Constraint::RowHasField(..) => 0,
            Constraint::RowFieldType(..) => 1,
            Constraint::Equal(..) => 2,
            Constraint::Subtype(..) | Constraint::StackEqual(..) => 3,
        }
    }

    /// Solve constraints by processing them in ascending [`weight`] order (see
    /// there for the priority tiers). A single **stable** sort of the input by
    /// weight yields the same result as separate ordered passes while keeping
    /// each kind's relative order.
    ///
    /// This is not a general fixpoint solver: it works because the
    /// dependencies form a clean linear chain (presence → types → definitions
    /// → relations). It also relies on `NewObj` rows being open, so the
    /// provisional records that presence constraints synthesize for
    /// still-unbound object variables reconcile with the definitional `Equal`s
    /// by row unification.
    pub fn solve(&mut self, constraints: &[Constraint]) -> Result<(), TypeError> {
        let mut ordered: Vec<&Constraint> = constraints.iter().collect();
        ordered.sort_by_key(|c| Self::weight(c));

        for c in ordered {
            match c {
                Constraint::RowHasField(row_ty, name) => {
                    self.check_row_has_field(row_ty.clone(), name.clone())?;
                }
                Constraint::RowFieldType(row_ty, name, field_ty) => {
                    self.check_row_field_type(row_ty.clone(), name.clone(), field_ty.clone())?;
                }
                Constraint::Equal(a, b) => self.unify(a.clone(), b.clone())?,
                Constraint::Subtype(a, b) => {
                    // resolve first so a record/interface referred to by a
                    // variable is recognized (and its row flattened)
                    let a = self.resolve_type(a.clone());
                    let b = self.resolve_type(b.clone());
                    match (a, b) {
                        (Type::Record(record_row), Type::Interface(interface_row)) => {
                            self.subtype_record_interface(record_row, interface_row)?
                        }
                        (a, b) => {
                            return Err(TypeError::UnificationFailed(format!(
                                "Subtype only supported for Record <: Interface, got {} <: {}",
                                a, b
                            )));
                        }
                    }
                }
                Constraint::StackEqual(s1, s2) => {
                    self.unify(Type::Stack(s1.clone()), Type::Stack(s2.clone()))?;
                }
            }
        }
        Ok(())
    }

    fn check_row_has_field(&mut self, row_ty: Type, name: String) -> Result<(), TypeError> {
        self.lookup_or_extend_field(row_ty, &name).map(|_| ())
    }

    /// Read-only field lookup: returns the field's type if the (resolved,
    /// flattened) row already has it, and `None` otherwise. Never extends the
    /// row or errors — presence is `RowHasField`'s responsibility.
    fn lookup_field(&self, row_ty: &Type, name: &str) -> Option<Type> {
        match self.resolve_type(row_ty.clone()) {
            Type::Record(row) | Type::Interface(row) => row.fields.get(name).cloned(),
            _ => None,
        }
    }

    /// Look up a field's type in a row type, extending the row via its tail
    /// when the field is missing and the row is open. An unbound variable is
    /// bound to a fresh open record containing just the field.
    fn lookup_or_extend_field(&mut self, row_ty: Type, name: &str) -> Result<Type, TypeError> {
        match self.resolve_type(row_ty) {
            Type::Record(row) | Type::Interface(row) => {
                if let Some(t) = row.fields.get(name) {
                    return Ok(t.clone());
                }

                // Missing field — try row-tail extension
                match row.tail {
                    Some(tail_tv) => {
                        let field_ty = Type::Unknown(self.tvg.fresh());

                        // Bind the old tail to a fragment holding the missing
                        // field, itself open via a fresh tail
                        let new_tail_tv = self.tvg.fresh_row();
                        let mut fragment = Row {
                            fields: BTreeMap::new(),
                            tail: Some(new_tail_tv),
                        };
                        fragment.fields.insert(name.to_string(), field_ty.clone());

                        self.bind_var(tail_tv, Type::Record(fragment))?;

                        Ok(field_ty)
                    }

                    None => Err(TypeError::UnificationFailed(format!(
                        "row missing field {:?} and row is closed",
                        name
                    ))),
                }
            }

            // Not yet known to be a record: bind to a fresh open record
            // containing just this field
            Type::Unknown(tv) => {
                let field_ty = Type::Unknown(self.tvg.fresh());
                let tail_tv = self.tvg.fresh_row();

                let mut row = Row {
                    fields: BTreeMap::new(),
                    tail: Some(tail_tv),
                };
                row.fields.insert(name.to_string(), field_ty.clone());

                self.bind_var(tv, Type::Record(row))?;

                Ok(field_ty)
            }

            other => Err(TypeError::UnificationFailed(format!(
                "field access {:?} on non-row type {:?}",
                name, other
            ))),
        }
    }

    /// Resolve a type by chasing variable substitutions; row types are
    /// additionally flattened via `resolve_row` so callers always see the
    /// fields accumulated through row-tail extension.
    pub fn resolve_type(&self, ty: Type) -> Type {
        let mut ty = ty;
        while let Type::Unknown(tv) = ty {
            match self.substitutions.get(&tv) {
                Some(next) => ty = next.clone(),
                None => return Type::Unknown(tv),
            }
        }

        match ty {
            Type::Record(row) => Type::Record(self.resolve_row(&row)),
            Type::Interface(row) => Type::Interface(self.resolve_row(&row)),
            other => other,
        }
    }

    /// Flatten a row by following the substitution chain of its tail,
    /// merging in the fields of every bound row fragment. Terminates at a
    /// closed (`None`) or still-unbound tail variable.
    pub fn resolve_row(&self, row: &Row) -> Row {
        let mut fields = row.fields.clone();
        let mut tail = row.tail;

        while let Some(tv) = tail {
            match self.substitutions.get(&tv) {
                Some(Type::Record(fragment)) => {
                    for (name, ty) in &fragment.fields {
                        fields.insert(name.clone(), ty.clone());
                    }
                    tail = fragment.tail;
                }
                Some(Type::Unknown(next)) => tail = Some(*next),
                // any other binding is a kind error caught in bind_var
                Some(_) | None => break,
            }
        }

        Row { fields, tail }
    }

    /// Apply the substitution deeply, resolving every variable inside the
    /// type and flattening rows. Useful for reporting final inferred types.
    pub fn apply(&self, ty: Type) -> Type {
        match self.resolve_type(ty) {
            Type::Int => Type::Int,
            Type::Bool => Type::Bool,
            Type::Unit => Type::Unit,
            Type::Ptr(inner) => Type::Ptr(Box::new(self.apply(*inner))),
            Type::Func(f) => Type::Func(FuncType {
                params: f.params.into_iter().map(|t| self.apply(t)).collect(),
                ret: Box::new(self.apply(*f.ret)),
                stack: f
                    .stack
                    .map(|s| s.into_iter().map(|t| self.apply(t)).collect()),
            }),
            Type::Record(row) => Type::Record(self.apply_row(row)),
            Type::Interface(row) => Type::Interface(self.apply_row(row)),
            Type::Stack(ts) => Type::Stack(ts.into_iter().map(|t| self.apply(t)).collect()),
            Type::Existential(e) => Type::Existential(Existential {
                var: e.var,
                ty: Box::new(self.apply(*e.ty)),
            }),
            Type::Unknown(tv) => Type::Unknown(tv),
        }
    }

    fn apply_row(&self, row: Row) -> Row {
        // resolve_type already flattened the row; still resolve field types
        Row {
            fields: row
                .fields
                .into_iter()
                .map(|(name, ty)| (name, self.apply(ty)))
                .collect(),
            tail: row.tail,
        }
    }

    fn check_row_field_type(
        &mut self,
        row_ty: Type,
        name: String,
        field_ty: Type,
    ) -> Result<(), TypeError> {
        // Only the field's type is constrained here; requiring the field to
        // exist is `RowHasField`'s job (paired with this for field access).
        // If the field is not present the constraint is vacuously satisfied.
        match self.lookup_field(&row_ty, &name) {
            Some(found) => self.unify(found, field_ty),
            None => Ok(()),
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
                let tail_tv = self.tvg.fresh_row();

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

            Instr::Const { dst, value } => {
                let ty = match value {
                    Value::Int(_) => Type::Int,
                    Value::Bool(_) => Type::Bool,
                    Value::Unit => Type::Unit,
                };
                vec![Constraint::Equal(dst.ty(), ty)]
            }

            Instr::BinOp { dst, op, lhs, rhs } => {
                // operands are always ints; the result is bool for comparisons
                let dst_ty = match op {
                    BinOpKind::Lt => Type::Bool,
                    _ => Type::Int,
                };
                vec![
                    Constraint::Equal(lhs.ty(), Type::Int),
                    Constraint::Equal(rhs.ty(), Type::Int),
                    Constraint::Equal(dst.ty(), dst_ty),
                ]
            }

            Instr::LoadFunc { dst, name: _, sig } => {
                // The runtime function's declared signature enters the
                // constraint system and unifies with the func type a later
                // Call synthesizes, so argument/return types flow both ways.
                vec![Constraint::Equal(dst.ty(), Type::Func(sig.clone()))]
            }

            Instr::Ret { .. } => {
                // The returned value's type flows from the body; it is bound to
                // the function's declared return type by the caller
                // (`solve_function`), not here.
                vec![]
            }

            Instr::If(f) => {
                let mut cs = vec![Constraint::Equal(f.cond.ty(), Type::Bool)];
                for instr in &f.then_.instrs {
                    cs.extend(self.generate_constraints(instr));
                }
                for instr in &f.else_.instrs {
                    cs.extend(self.generate_constraints(instr));
                }
                // Merge: whichever branch runs, its result flows into `dst`.
                // Pushed after the block constraints so the stable weight-sort
                // keeps "result defined before merged".
                cs.push(Constraint::Equal(f.dst.ty(), f.then_.result.ty()));
                cs.push(Constraint::Equal(f.dst.ty(), f.else_.result.ty()));
                cs
            }

            Instr::For(f) => {
                let mut cs = vec![
                    Constraint::Equal(f.index.ty(), Type::Int),
                    Constraint::Equal(f.bound.ty(), Type::Int),
                    // Accumulator is seeded from `init`.
                    Constraint::Equal(f.acc.ty(), f.init.ty()),
                ];
                for instr in &f.body.instrs {
                    cs.extend(self.generate_constraints(instr));
                }
                // Checked loop invariant: the body's yielded value must have the
                // same type as the accumulator (a plain `Equal`, not a
                // fixpoint).
                cs.push(Constraint::Equal(f.acc.ty(), f.body.result.ty()));
                cs
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InstructionBuilder;

    /// Generate constraints for the builder's body (plus any extras) and
    /// solve them, returning the solver for inspection.
    fn solve_with(
        builder: &mut InstructionBuilder,
        extra: Vec<Constraint>,
    ) -> Result<Solver<'_>, TypeError> {
        let body = builder.body.clone();
        let mut solver = Solver::new(&mut builder.type_variable_generator);

        let mut constraints = Vec::new();
        for instr in &body {
            constraints.extend(solver.generate_constraints(instr));
        }
        constraints.extend(extra);

        solver.solve(&constraints)?;
        Ok(solver)
    }

    fn solve(builder: &mut InstructionBuilder) -> Result<Solver<'_>, TypeError> {
        solve_with(builder, Vec::new())
    }

    fn as_record(ty: Type) -> Row {
        match ty {
            Type::Record(row) => row,
            other => panic!("expected record type, got {:?}", other),
        }
    }

    /// PLAN.md headline case: loading a missing field from an open record
    /// extends the row through its tail instead of failing.
    #[test]
    fn load_missing_field_extends_open_row() {
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);
        let r_y = b.load(obj, "y");

        let solver = solve(&mut b).expect("row-tail extension should solve");

        let row = as_record(solver.apply(obj.ty()));
        assert!(row.fields.contains_key("x"));
        assert!(row.fields.contains_key("y"));
        assert!(row.tail.is_some(), "extended row must remain open");
        assert!(row.tail.unwrap().is_row());

        // the new field's type is the load destination's type
        assert_eq!(
            solver.apply(row.fields["y"].clone()),
            solver.apply(r_y.ty())
        );
    }

    #[test]
    fn load_existing_field_unifies_with_dst() {
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let r_y = b.reg();
        let obj = b.new_obj(vec![("x", r_x), ("y", r_y)]);
        let dst = b.load(obj, "y");

        let solver = solve(&mut b).expect("should solve");

        assert_eq!(solver.apply(dst.ty()), solver.apply(r_y.ty()));
    }

    #[test]
    fn store_missing_field_extends_open_row() {
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);
        let r_z = b.reg();
        b.store(obj, "z", r_z);

        let solver = solve(&mut b).expect("should solve");

        let row = as_record(solver.apply(obj.ty()));
        assert!(row.fields.contains_key("z"));
        assert_eq!(
            solver.apply(row.fields["z"].clone()),
            solver.apply(r_z.ty())
        );
    }

    #[test]
    fn row_field_type_alone_does_not_create_field() {
        // A standalone RowFieldType on an absent field is a no-op: it neither
        // extends the row nor errors. (Presence is RowHasField's job.)
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);

        let solver = solve_with(
            &mut b,
            vec![Constraint::RowFieldType(
                obj.ty(),
                "y".to_string(),
                Type::Int,
            )],
        )
        .expect("field-type constraint on absent field should be vacuous");

        let row = as_record(solver.apply(obj.ty()));
        assert!(row.fields.contains_key("x"));
        assert!(
            !row.fields.contains_key("y"),
            "RowFieldType must not create the field"
        );
    }

    #[test]
    fn row_has_field_alone_creates_field() {
        // The complementary case: RowHasField *does* extend the open row.
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);

        let solver = solve_with(
            &mut b,
            vec![Constraint::RowHasField(obj.ty(), "y".to_string())],
        )
        .expect("presence constraint should extend the open row");

        let row = as_record(solver.apply(obj.ty()));
        assert!(
            row.fields.contains_key("y"),
            "RowHasField must create the field"
        );
    }

    #[test]
    fn field_constraints_are_order_independent() {
        // RowFieldType listed *before* the RowHasField that creates the field:
        // staged solving settles all presence first, so `y` is still typed int.
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);

        let solver = solve_with(
            &mut b,
            vec![
                Constraint::RowFieldType(obj.ty(), "y".to_string(), Type::Int),
                Constraint::RowHasField(obj.ty(), "y".to_string()),
            ],
        )
        .expect("should solve regardless of constraint order");

        let row = as_record(solver.apply(obj.ty()));
        assert_eq!(
            solver.apply(row.fields["y"].clone()),
            Type::Int,
            "field type must be applied even though RowFieldType came first"
        );
    }

    #[test]
    fn subtype_wider_record_satisfies_interface() {
        // width subtyping: { x, y } <: { x: int }, and the check drives x to int
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let r_y = b.reg();
        let obj = b.new_obj(vec![("x", r_x), ("y", r_y)]);

        let iface = Type::Interface(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int)]),
            tail: None,
        });

        let solver = solve_with(&mut b, vec![Constraint::Subtype(obj.ty(), iface)])
            .expect("wider record should satisfy interface");

        // the interface's x: int flowed back into the object's field
        assert_eq!(solver.apply(r_x.ty()), Type::Int);
    }

    #[test]
    fn subtype_rejects_missing_interface_field() {
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let obj = b.new_obj(vec![("x", r_x)]);

        let iface = Type::Interface(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int), ("y".to_string(), Type::Int)]),
            tail: None,
        });

        let result = solve_with(&mut b, vec![Constraint::Subtype(obj.ty(), iface)]);
        assert!(matches!(result, Err(TypeError::UnificationFailed(_))));
    }

    #[test]
    fn subtype_before_defining_equal_still_solves() {
        // The `Subtype` is listed *before* the `Equal` that defines the
        // object's record. Weighted solving runs `Equal` (weight 2) before
        // `Subtype` (weight 3), so the record is defined by the time the
        // subtype check runs — the list order does not matter.
        let mut tvg = TypeVarGenerator::default();
        let obj_tv = tvg.fresh();
        let mut solver = Solver::new(&mut tvg);

        let record = Type::Record(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int), ("y".to_string(), Type::Int)]),
            tail: None,
        });
        let iface = Type::Interface(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int)]),
            tail: None,
        });

        let constraints = vec![
            // relational check listed first...
            Constraint::Subtype(Type::Unknown(obj_tv), iface),
            // ...but its definition comes later in the list
            Constraint::Equal(Type::Unknown(obj_tv), record),
        ];

        solver
            .solve(&constraints)
            .expect("Equal must run before Subtype regardless of list order");
    }

    #[test]
    fn open_rows_unify_by_absorbing_exclusive_fields() {
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let r_y1 = b.reg();
        let obj1 = b.new_obj(vec![("x", r_x), ("y", r_y1)]);
        let r_y2 = b.reg();
        let r_z = b.reg();
        let obj2 = b.new_obj(vec![("y", r_y2), ("z", r_z)]);

        let solver = solve_with(&mut b, vec![Constraint::Equal(obj1.ty(), obj2.ty())])
            .expect("open rows should unify");

        for obj in [obj1, obj2] {
            let row = as_record(solver.apply(obj.ty()));
            assert!(row.fields.contains_key("x"));
            assert!(row.fields.contains_key("y"));
            assert!(row.fields.contains_key("z"));
            assert!(row.tail.is_some(), "merged row must remain open");
        }

        // shared field types unified
        assert_eq!(solver.apply(r_y1.ty()), solver.apply(r_y2.ty()));
    }

    #[test]
    fn closed_row_rejects_missing_field() {
        let mut tvg = TypeVarGenerator::default();
        let mut solver = Solver::new(&mut tvg);

        let closed_x = Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int)]),
            tail: None,
        };
        let closed_xy = Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int), ("y".to_string(), Type::Bool)]),
            tail: None,
        };

        let result = solver.unify_row(closed_x, closed_xy);
        assert!(matches!(result, Err(TypeError::UnificationFailed(_))));
    }

    #[test]
    fn field_lookup_on_closed_row_fails() {
        let mut tvg = TypeVarGenerator::default();
        let obj_tv = tvg.fresh();
        let mut solver = Solver::new(&mut tvg);

        let closed = Type::Record(Row {
            fields: BTreeMap::from([("x".to_string(), Type::Int)]),
            tail: None,
        });
        solver
            .unify(Type::Unknown(obj_tv), closed)
            .expect("binding should succeed");

        let result = solver.solve(&[Constraint::RowHasField(
            Type::Unknown(obj_tv),
            "y".to_string(),
        )]);
        assert!(matches!(result, Err(TypeError::UnificationFailed(_))));
    }

    #[test]
    fn row_var_cannot_unify_with_plain_type() {
        let mut tvg = TypeVarGenerator::default();
        let row_var = tvg.fresh_row();
        let mut solver = Solver::new(&mut tvg);

        let result = solver.unify(Type::Unknown(row_var), Type::Int);
        assert!(matches!(result, Err(TypeError::KindMismatch(_))));
    }

    #[test]
    fn call_infers_function_type() {
        // example0 flow: obj construction, field load, call
        let mut b = InstructionBuilder::default();
        let r_x = b.reg();
        let r_y = b.reg();
        let obj = b.new_obj(vec![("x", r_x), ("y", r_y)]);
        let r_z = b.load(obj, "x");
        let r_f = b.reg();
        let r_ret = b.call(r_f, vec![obj, r_z]);

        let solver = solve(&mut b).expect("should solve");

        match solver.apply(r_f.ty()) {
            Type::Func(f) => {
                assert_eq!(f.params.len(), 2);
                assert!(matches!(f.params[0], Type::Record(_)));
                assert_eq!(*f.ret, solver.apply(r_ret.ty()));
            }
            other => panic!("expected function type, got {:?}", other),
        }
    }
}
