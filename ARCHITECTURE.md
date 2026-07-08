# Typed IR Architecture

This document is the canonical technical reference for the repository.

## 1. System Overview

The project has four layers:

1. IR construction (`InstructionBuilder` / `FunctionBuilder` + `Instr`)
2. Constraint generation (`infer::constraints_for`)
3. Constraint solving / type inference (`Solver`)
4. Code emission (`Solved::generate_code`)

The inference-to-C flow is a **type-state pipeline** in `src/infer.rs`, where each
stage carries its result as a public field and consumes itself into the next:

```
Inference::new(body, registers)
    .generate_constraints(tvg)   // -> Constraints { constraints }
    .solve(tvg)?                 // -> Solved { solver }
    .generate_code()?            // -> C source (String)
```

The build/run flow is the analogous outer pipeline in `src/build.rs`:
`CBuild::…generate()? -> Source -> .compile()? -> Compiled -> .run()? -> RunReport`.
Consuming stages mean a mis-ordered call is a compile error and nothing re-runs.

## 2. Core Data Model

### 2.1 Type Variables

- `TypeVar` is an immutable id for unknown types.
- `TypeVarGenerator` creates fresh type variables.
- Row-kind vars are tagged in the id (`fresh_row()`, `is_row()`).

### 2.2 Types

`Type` includes:

- `Int`, `Bool`
- `Unit` — the singleton type (one value); lowers to a one-byte C `unit_t`
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
- `InstructionBuilder` allocates registers and appends instructions.
- `FunctionBuilder<N>` wraps an `InstructionBuilder` with a typed, compile-time-arity
  signature (it reserves `reg0..regN` as parameters) and reuses the whole
  instruction-emitting surface via `Deref`.
- Builders are intentionally untyped: all type structure comes from constraint generation.

## 3. Instruction Set

Current instruction variants:

- `Load { dst, src, field }`
- `Store { dst, field, src }`
- `NewObj { dst, fields }`
- `Call { func, args, ret: Option<Reg> }` — a `None` `ret` is a void call (no result register)
- `Const { dst, value }` — `value` is `Int` / `Bool` / `Unit`
- `BinOp { dst, op, lhs, rhs }`
- `LoadFunc { dst, name, sig }`
- `Ret { src: Option<Reg> }` — a valueless `ret` returns unit
- `If(If)` / `For(For)` — control flow; see §3.1

`LoadFunc` injects function signatures into inference. Each `ret` is bound to the
enclosing function's declared return type (by `Inference::for_function`).

### 3.1 Structured control flow

`If` and `For` are **combinator instructions** (newtype variants over the `If` /
`For` structs): they carry `Block { instrs, result }` sub-blocks rather than
branching to labels, so the IR stays a tree and typing stays syntax-directed (no
code-label preconditions, no dataflow fixpoint). Both are value-producing:

- `If { cond, then_: Block, else_: Block, dst }` merges the two branch results
  into `dst` with `Equal(dst, then_.result)` + `Equal(dst, else_.result)`.
- `For { index, bound, acc, init, body: Block }` carries an accumulator: `acc`
  is seeded from `init` and set to `body.result` each iteration. The loop
  invariant is **checked**, not inferred — a plain `Equal(acc, body.result)`,
  never a fixpoint. `index` runs `0..bound`.

Every constraint these emit is `Equal`, so the weighted solver is unchanged.
Restrictions: block-local registers do not escape their block (values leave only
via `dst`/`acc` or heap stores), and `Ret` is not permitted inside a block.

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

Constraint **generation** lives in `src/infer.rs` as free functions
(`constraints_from_instr` / `constraints_for`), which need only the
`TypeVarGenerator` (to mint a `NewObj`'s fresh row tail). `Solver` is the
**solving** engine: it owns substitutions and uses the shared `TypeVarGenerator`.

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

A single solved body is lowered by `Solved::generate_code` (the tail of the inner
pipeline). A whole multi-function `Program` is assembled by `codegen::emit_code`
(crate-private, driven by `CBuild::from_program`): it mangles names, emits a
preamble, per-function prototypes, the prelude, per-function definitions (each
solved via `Inference::for_function`), and a `main` wrapper.

Type lowering:

- `Int` -> `int64_t`
- `Bool` -> `bool`
- `Unit` -> `unit_t` (a one-byte singleton struct; the `UNIT` macro is its value)
- `Ptr(t)` -> `<t>*`
- `Record(row)` -> `struct Rn *` (heap object; `NewObj` uses `calloc`)
- `Func(ft)` -> interned function-pointer typedef (`fnN`)

Struct shapes are structurally deduplicated. Open row tails are closed at codegen time and noted in comments.

Runtime functions come from the `prelude` table (`src/prelude.rs`), the single
source of each function's name, signature, and C body:

- Prelude names (`print_int`, `print_bool`) are pulled in via `builder.prelude(name)`
  and their C bodies emitted when loaded.
- Other loaded functions receive `extern` prototypes.

Error policy:

- `CodegenError` — per-body lowering: unresolved types (`UnresolvedType`) and
  unsupported *types* (`Unsupported`: `Interface` / `Existential` / `Stack`, …).
- `CompilerError` — whole-program assembly: `MissingEntry`, `DuplicateFunction`,
  `UnsupportedProgram` (an unsupported program *construct*), plus `Type(TypeError)`
  and `Codegen(CodegenError)` wrapping the lower-layer failures.

### 7.1 Build harness

`build::CBuild` ties codegen to a C toolchain as a type-state pipeline. Construct
with `from_body` (a solved body), `from_program` (a `Program`), or `from_builder`
(runs the whole inner pipeline for you), tweak `dir`/`cc`/`flags`, then advance:
`generate()` → `Source` (the C, still in memory), `Source::compile()` → `Compiled`
(writes `<dir>/<name>.c` and invokes `cc -Wall -O2`), `Compiled::run()` →
`RunReport` (stdout/stderr/exit status). Errors unify under `BuildError`
(codegen / io / non-zero compile).

## 8. What Is Implemented vs Open

Implemented:

- Row-polymorphic inference with row-tail extension.
- Register-level type inference across calls/loads/stores/binops.
- C backend and runnable examples.
- Feature-gated ASCII/Unicode displays.

Known limitations:

- Existential unification is not implemented.
- Subtyping is currently focused on `Record <: Interface`.
