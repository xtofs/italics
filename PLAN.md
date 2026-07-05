# PLAN: Row‑Polymorphic Type Inference — Open Records, Row Tails, and Row‑Tail Extension

## 1. Goal

Implement full row‑polymorphic record typing in the IR + constraint solver pipeline. This includes:

- Open record generation in `generate_constraints`
- Row tail creation and kind assignment
- RowHasField + RowFieldType constraints
- Row‑tail extension in the solver
- End‑to‑end inference test demonstrating row‑tail extension

The IRBuilder remains untyped. All type construction happens in `generate_constraints`.

---

## 2. Architectural Invariants

### 2.1 IRBuilder

- Creates registers and instructions only.
- Does not create types.
- Does not create row tails.
- Does not assign kinds.
- Every register gets a fresh `TypeVar` from the shared `TypeVarGenerator`.

### 2.2 TypeVarGenerator

- Single shared instance for the entire pipeline.
- Used by IRBuilder and Solver.
- Generates all type variables (normal and row‑kind).

### 2.3 Constraint Generator

Responsible for creating:

- Open record types
- Row tails
- Function types
- Interface types
- All constraints

### 2.4 Solver

Responsible for:

- Unification
- Row‑tail extension
- Row‑tail unification
- Subtyping
- Kind checking

Solver must use the same `TypeVarGenerator` as IRBuilder.

---

## 3. Required Changes (implemented)

### 3.1 Fresh-variable helpers on Solver

Kinds are carried by a tag bit on `TypeVar` itself (`vars.rs`: `fresh_row()`
sets bit 31, `TypeVar::is_row()`), replacing the earlier `kinds:
HashMap<TypeVar, Kind>` design.

```rust
fn fresh_tv(&mut self) -> TypeVar {
    self.tvg.fresh()
}

fn fresh_row_tail_var(&mut self) -> TypeVar {
    let tv = self.tvg.fresh_row();
    self.row_tail_vars.insert(tv);
    tv
}
```

### 3.2 Row flattening (`resolve_row`)

Row-tail extension binds a tail variable to a row *fragment*
(`Record { new fields | fresh tail }`). `resolve_row` follows that chain and
merges the fields, so every consumer (unification, field constraints,
reporting) sees the full accumulated row. `resolve_type` chases variable
substitutions and flattens row types.

### 3.3 Field lookup with extension (`lookup_or_extend_field`)

`RowHasField` and `RowFieldType` share one helper:

- field present → return its type
- field missing, row open → bind the tail to a fragment holding the field
  (fresh type var) plus a fresh tail; return the fresh field type
- field missing, row closed → type error
- type still an unbound variable → bind it to a fresh open record containing
  just the field (makes constraint order less brittle)

### 3.4 Open-row unification (`unify_row`)

Rémy-style: shared fields unify pointwise; fields exclusive to one side are
absorbed into the other side's tail. Both-open rows share a fresh common
tail; a closed row missing fields is a type error; identical tails cannot
absorb differing fields.

### 3.5 Kind checking (`bind_var` / `check_kind`)

A row variable may only be bound to a row fragment or another row variable;
a type variable may never be bound to a row variable
(`TypeError::KindMismatch`).

### 3.6 End-to-end tests

`src/solver.rs` has a test module driving the full pipeline (IRBuilder →
`generate_constraints` → `solve`), including the headline row-tail-extension
case: `new_obj {x}` followed by `load obj.y` solves, and the object's type
becomes `record { x, y | ρ }`.
