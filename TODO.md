# Typed IR Compiler TODO / Plan

## 1. Type unification engine

- **Implement `Solver::unify` properly**
  - Handle `Unknown(TypeVar)`:
    - If one side is `Unknown(tv)` and the other is concrete, bind `tv` to the concrete type.
    - If both sides are `Unknown(tv1, tv2)`, unify their substitutions.
  - Handle `Int`, `Bool`, `Ptr`:
    - `Ptr` requires recursive unification of the pointee type.
  - Handle `Func(FuncType)`:
    - Unify parameter lists element-wise.
    - Unify return types.
    - Unify optional stack typing if present.
  - Handle `Record(Row)` and `Interface(Row)`:
    - Row unification:
      - unify fields with same names.
      - handle tails (`tail: Option<TypeVar>`) for open rows.
  - Handle `Existential(Existential)`:
    - Respect scoping of existential type variables.
    - Unify existential packages only when compatible.
  - Handle `Stack(Vec<Type>)`:
    - Unify element-wise.

## 2. Row polymorphism and interfaces

- **Row unification**
  - Implement support for:
    - closed rows (no tail)
    - open rows (tail `Some(TypeVar)`)
  - `Subtype(Type::Record, Type::Interface)`:
    - record row must contain at least the fields of the interface row.
    - unify field types accordingly.
  - `RowHasField` and `RowFieldType`:
    - integrate with row unification:
      - if field missing, either:
        - add it to an open row via tail variable, or
        - report type error for closed rows.

- **Interface modeling**
  - Decide whether `Interface(Row)` is:
    - purely structural (current design), or
    - later extended with nominal IDs.
  - Add constraints for interface satisfaction:
    - `Subtype(Record(row_obj), Interface(row_iface))`.

## 3. Existential types

- **Existential packages**
  - Add instructions for:
    - `pack` — create existential package from a concrete type and value.
    - `unpack` — open existential package with a fresh type variable.
  - Constraint generation:
    - `Existential(Existential)` must hide the internal `TypeVar`.
    - Unification must respect existential boundaries.

## 4. Stack typing

- **Stack representation**
  - Use `Type::Stack(Vec<Type>)` for stack shapes.
  - Add instructions for:
    - push/pop
    - call/return with stack effects.
  - Constraints:
    - `StackEqual` for matching caller/callee stack shapes.
    - integrate with `FuncType.stack`.

## 5. IR extensions

- **More instructions**
  - Arithmetic: `Add`, `Sub`, `Mul`, `Div`:
    - constraints: operands and result must be `Int` (or numeric types later).
  - Control flow:
    - `Branch`, `Jump`, `Phi` (if SSA):
      - constraints for merging types at join points.
  - Memory:
    - `LoadPtr`, `StorePtr` for `Ptr` types.

- **SSA support (optional)**
  - Extend `RegId` to encode SSA versions.
  - Add builder helpers for phi nodes.

## 6. Backend: C / LLVM

- **Type mapping**
  - Map `Type` to C/LLVM types:
    - `Int` → `int` / `i32`
    - `Bool` → `_Bool` / `i1`
    - `Ptr(t)` → `t*` / `ptr`
    - `Record(Row)` → `struct` / `%struct`
    - `Func(FuncType)` → function signatures.

- **Code generation**
  - For each function:
    - use solved types from `Solver` to:
      - emit struct definitions for records.
      - emit function prototypes.
      - emit function bodies using registers as temporaries.

## 7. Testing and debugging

- **Pretty-printing**
  - Implement pretty-printers for:
    - `Type`
    - `Row`
    - `Constraint`
    - IR (`Instr`, `InstructionBuilder.body`).

- **Unit tests**
  - Small programs:
    - simple object construction + field access.
    - function calls with inferred signatures.
    - interface-like constraints via `Subtype`.

- **Error reporting**
  - Improve `TypeError`:
    - include source-level context (later).
    - show conflicting constraints.

---

## 8. Longer-term directions

- **Row calculus alignment**
  - Align row polymorphism with formal row calculi (Rémy, MLPolyR).
- **iTalX-style global inference**
  - Extend from per-function to whole-program inference if needed.
- **Wasm / LLVM integration**
  - Explore emitting typed Wasm or richer LLVM IR with metadata.

This plan takes the current skeleton to a usable, TAL/iTalX-inspired typed compiler that can infer types, model objects and interfaces structurally, and eventually emit C or LLVM.
