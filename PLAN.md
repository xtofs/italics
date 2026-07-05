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

## 3. Required Changes

### 3.1 Add `fresh_tv(kind)` to Solver

```rust
fn fresh_tv(&mut self, kind: Kind) -> TypeVar {
    let tv = self.tvg.fresh();
    self.kinds.insert(tv, kind);
    tv
}
```
