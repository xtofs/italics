# Typed IR Architecture

This document is the canonical technical reference for the repository.

## 1. System Overview

The project has four layers:

1. IR construction (`IRBuilder` + `Instr`)
2. Constraint generation (`Solver::generate_constraints`)
3. Constraint solving / type inference (`Solver`)
4. Code emission (`codegen::emit_c`)

High-level flow:

1. Build a register-based IR program.
2. Generate constraints from each instruction.
3. Solve constraints to infer register types.
4. Lower solved types and instructions into runnable C.

## 2. Core Data Model

### 2.1 Type Variables

- `TypeVar` is an immutable id for unknown types.
- `TypeVarGenerator` creates fresh type variables.
- Row-kind vars are tagged in the id (`fresh_row()`, `is_row()`).

### 2.2 Types

`Type` includes:

- `Int`, `Bool`
- `Ptr(Box<Type>)`
- `Func(FuncType)`
- `Record(Row)`
- `Interface(Row)`
- `Existential(Existential)`
- `Stack(Vec<Type>)`
- `Unknown(TypeVar)`

`Row` is `{ fields, tail }`, where `tail` represents openness (row polymorphism).

### 2.3 Registers and Builder

- Registers (`Reg`) carry an id and a type variable handle.
- `IRBuilder` allocates registers and appends instructions.
- Builder is intentionally untyped: all type structure comes from constraint generation.

## 3. Instruction Set

Current instruction variants:

- `Load { dst, src, field }`
- `Store { dst, field, src }`
- `NewObj { dst, fields }`
- `Call { func, args, ret }`
- `Const { dst, value }`
- `BinOp { dst, op, lhs, rhs }`
- `LoadFunc { dst, name, sig }`
- `Ret { src }`

`LoadFunc` injects function signatures into inference, and `Ret` defines the observable program result.

## 4. Constraints and Meaning

Constraint kinds:

- `Equal(Type, Type)`
- `RowHasField(Type, String)`
- `RowFieldType(Type, String, Type)`
- `Subtype(Type, Type)`
- `StackEqual(Vec<Type>, Vec<Type>)`

`RowHasField` and `RowFieldType` are intentionally split:

- `RowHasField` handles field presence (and open-row extension).
- `RowFieldType` links a field's type to another type.

Together they model `load`/`store` access without conflating shape and type constraints.

## 5. Solver Design

`Solver` owns substitutions and uses the shared `TypeVarGenerator`.

Key behavior:

- Occurs-check and kind checking during variable binding.
- Row flattening (`resolve_row`) across tail-fragment chains.
- Row extension when a required field is missing on an open row.
- Rémy-style row unification for open records.

### 5.1 Weighted Solve Order

Constraints are solved by stable sort on weight:

1. `RowHasField` (presence)
2. `RowFieldType` (field type links)
3. `Equal` (definitional structure)
4. `Subtype` / `StackEqual` (relational checks)

This is not a general fixpoint solver; it works because dependencies are linear.

## 6. Display and Formatting Modes

Display formatting is centralized in `src/display.rs`.

- Default mode (ASCII): `t_0`, `r_3`, `->`, `:in:`, `<:`, `=>`
- `pretty-unicode` feature mode: `τ₀`, `ρ₃`, `→`, `∈`, `⊆`, `↦`

Type and constraint displays, plus generated-code comments, share this policy.

## 7. Code Generation

`emit_c(body, registers, solver)` lowers inferred programs to C.

Type lowering:

- `Int` -> `int64_t`
- `Bool` -> `bool`
- `Ptr(t)` -> `<t>*`
- `Record(row)` -> `struct Rn *` (heap object; `NewObj` uses `calloc`)
- `Func(ft)` -> interned function-pointer typedef (`fnN`)

Struct shapes are structurally deduplicated. Open row tails are closed at codegen time and noted in comments.

Runtime functions:

- Known prelude names (`print_int`, `print_bool`) are emitted inline.
- Unknown loaded functions receive `extern` prototypes.

Error policy:

- Unresolved types are hard errors (`CodegenError::UnresolvedType`).
- Unsupported targets (`Interface`, `Existential`, `Stack`) are explicit errors.

## 8. What Is Implemented vs Open

Implemented:

- Row-polymorphic inference with row-tail extension.
- Register-level type inference across calls/loads/stores/binops.
- C backend and runnable examples.
- Feature-gated ASCII/Unicode displays.

Known limitations:

- Existential unification is not implemented.
- Subtyping is currently focused on `Record <: Interface`.
