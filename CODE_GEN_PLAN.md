# C Code Generation from Inferred Types

## Context

The row-polymorphic inference engine is complete (weighted-sort solver, 14 passing tests, 7 examples). The point of inferring types was always to _use_ them — ARCHITECTURE.md §4 ends with "Later: use solved types to emit C or LLVM IR." This plan adds that step: a C backend that demonstrates the payoff of inference, most visibly that **a struct definition contains fields the program never constructed explicitly** — fields added by row-tail extension from later `load`/`store` instructions.

Decisions made with the user:

- **C target** for readability/debuggability (other targets may come later — keep the codegen module self-contained so it can be swapped).
- **Records = heap pointers**: `NewObj` → `calloc`, registers hold `struct Rn *`. Aliasing-correct; calloc zero-fills row-extended fields never stored.
- **Runnable output**: emit a prelude defining known runtime functions (`print_int`, `print_bool`) so `cc out.c && ./a.out` works; unknown `LoadFunc` names get `extern` prototypes.
- **New instructions**: `Const`, `LoadFunc`, `Ret`, and `BinOp` (arithmetic). `Const` is required so anything grounds to `int`/`bool`; `LoadFunc` is the requested mechanism for loading runtime-defined functions into a register with their signature expressed as a constraint.
- **Unresolved types are a codegen error** (`CodegenError::UnresolvedType` naming the register/var) — defaulting would hide inference gaps, the opposite of the demo's purpose. Exception: a still-open row _tail_ is silently closed at codegen time (width polymorphism collapses to the inferred width; note it in a struct comment).

## 1. New instructions — [src/instructions.rs](src/instructions.rs)

```rust
#[derive(Debug, Clone, Copy)]
pub enum Value { Int(i64), Bool(bool) }

#[derive(Debug, Clone, Copy)]
pub enum BinOpKind { Add, Sub, Mul, Lt }   // Lt: (int,int) -> bool

// added to Instr:
Const   { dst: Reg, value: Value },
BinOp   { dst: Reg, op: BinOpKind, lhs: Reg, rhs: Reg },
LoadFunc{ dst: Reg, name: String, sig: FuncType },
Ret     { src: Reg },
```

Extend `Display for Instr` in the existing style: `const reg0 = 42`, `add  reg2 = reg0 + reg1`, `func reg3 = @print_int : (int) → int`, `ret  reg4`.

## 2. Builder methods — [src/builder.rs](src/builder.rs)

Follow the existing method pattern (allocate dst via `self.reg()`, push instr, return dst):

```rust
pub fn const_int(&mut self, v: i64) -> Reg
pub fn const_bool(&mut self, v: bool) -> Reg
pub fn binop(&mut self, op: BinOpKind, lhs: Reg, rhs: Reg) -> Reg
pub fn func(&mut self, name: impl Into<String>, params: Vec<Type>, ret: Type) -> Reg  // LoadFunc
pub fn ret(&mut self, src: Reg)
```

`func` builds the `FuncType { params, ret, stack: None }` itself — this is the "describe the signature as constraints" entry point.

## 3. Constraint generation — [src/solver.rs](src/solver.rs) `generate_constraints`

New arms (all reuse `Constraint::Equal`, weight 2 — no new constraint kinds or solver changes needed):

- `Const` → `Equal(dst.ty(), Int)` or `Equal(dst.ty(), Bool)` per `Value`.
- `BinOp` → `Equal(lhs.ty(), Int)`, `Equal(rhs.ty(), Int)`, `Equal(dst.ty(), Int)` (dst is `Bool` for `Lt`).
- `LoadFunc` → `Equal(dst.ty(), Func(sig.clone()))` — the signature enters the constraint system and unifies with the `Equal` that a later `Call` on the same register generates, so argument/return types flow both ways (e.g. `print_int`'s `(int) → int` forces the loaded field to `int`).
- `Ret` → `Equal(src.ty(), Int)` (program result is the exit-observable int).

## 4. Codegen module — new [src/codegen.rs](src/codegen.rs)

```rust
pub enum CodegenError {
    UnresolvedType(String),   // names the register and the τ var
    Unsupported(String),      // Interface/Existential/Stack-typed register, etc.
}

pub fn emit_c(body: &[Instr], registers: &RegisterFile, solver: &Solver) -> Result<String, CodegenError>
```

Internally a small `CodeGen` struct holding interning tables. Passes:

1. **Ground register types.** For every register: `solver.apply(reg.ty())` (existing public API — no solver changes).
2. **Lower `Type` → C type string**, interning as it goes:
   - `Int` → `int64_t`, `Bool` → `bool`, `Ptr(t)` → `<t>*`.
   - `Record(row)` → `struct R<n> *`. Structural dedup: key = the lowered `(name, ctype)` field list (the `BTreeMap` already sorts field names); identical shapes share one struct. Field types are lowered depth-first, so nested records get smaller ids and plain id-order emission is dependency-correct (occurs check guarantees no recursive types). An open tail is dropped with a `/* closed from ρn */` comment in the struct def.
   - `Func(ft)` → interned function-pointer `typedef` (`typedef int64_t (*fn<n>)(struct R0 *, int64_t);`).
   - `Unknown(tv)` (non-row) → `CodegenError::UnresolvedType`.
   - `Interface`/`Existential`/`Stack` → `Unsupported`.
3. **Emit**, in order:
   - includes: `stdint.h`, `stdbool.h`, `stdio.h`, `stdlib.h`
   - prelude: `static int64_t print_int(int64_t x) { printf("%lld\n", ...); return x; }`, same for `print_bool` (runtime fns return their argument — the type algebra has no unit type, and adding one is out of scope)
   - struct defs in interning order, then fn-ptr typedefs
   - `extern` prototypes for `LoadFunc` names not in the prelude
   - `int main(void)` — one statement per instruction, registers declared at first definition (C99), names `reg<n>` matching the IR `Display`:

   | Instr      | C                                                              |
   | ---------- | -------------------------------------------------------------- |
   | `Const`    | `int64_t reg0 = 42;`                                           |
   | `NewObj`   | `struct R0 *reg2 = calloc(1, sizeof *reg2); reg2->x = reg0;`   |
   | `Load`     | `int64_t reg3 = reg2->x;`                                      |
   | `Store`    | `reg2->y = reg3;` (no decl — no dst def)                       |
   | `BinOp`    | `int64_t reg4 = reg0 + reg3;`                                  |
   | `LoadFunc` | `fn0 reg5 = print_int;`                                        |
   | `Call`     | `int64_t reg6 = reg5(reg4);`                                   |
   | `Ret`      | `printf("result: %lld\n", (long long)reg4); return (int)reg4;` |

   If no `Ret` appears, end with `return 0;`.

Register in [src/lib.rs](src/lib.rs): `pub mod codegen;` + `pub use codegen::{emit_c, CodegenError};`. (Leave the empty, un-wired `src/vm.rs` untouched.)

## 5. Example — new [examples/codegen.rs](examples/codegen.rs)

One end-to-end program chosen to show inference driving layout:

```text
n    = const 42
one  = const 1
obj  = new { x: n }              // struct starts as { x }
m    = load obj.y                // row extension: struct R0 gains y (never constructed!)
sum  = add m, one                // forces y : int
f    = func @print_int : (int) → int
_    = call f(sum)
store obj.z = sum                // row extension again: struct gains z
ret sum
```

The example prints the IR body, the constraints, the solved register types (reusing the reporting style of [examples/main.rs](examples/main.rs)'s `run`), then the generated C — and writes it to `target/generated.c` so the verification step below can compile it. The headline observable: `struct R0 { int64_t x; int64_t y; int64_t z; /* closed from ρn */ };` where `y`/`z` exist purely through inference.

## 6. Tests — `#[cfg(test)]` in src/codegen.rs

Drive the real pipeline (IRBuilder → generate_constraints → solve → emit_c), mirroring the solver test helpers:

- **Row-extended field reaches the struct**: `new {x}` + `store obj.y` → emitted C contains a struct def with both `x` and `y`.
- **Structural dedup**: two `new_obj`s with identical shapes → exactly one struct definition.
- **LoadFunc signature drives inference**: `func @print_int : (int) → int` + `call f(load obj.x)` → obj's `x` lowers to `int64_t`, and the extern/typedef line matches the signature.
- **Unresolved type is an error**: a register with a never-constrained τ → `CodegenError::UnresolvedType`.
- **Unsupported type is an error**: an interface-typed register → `Unsupported`.
- **Compile-and-run smoke test** (`#[ignore]`d, run manually / in verification): write emitted C to a temp file, `cc` it, execute, assert stdout is `42\n…` etc.

## 7. Doc sync

- ARCHITECTURE.md: extend §1 instruction list with the four new instructions; new **§5 Code generation** describing type lowering (records → deduped structs behind pointers, open tails closed at codegen), the runnable prelude, and the unresolved-type policy; update §4's "Later: emit C" to reflect it exists.
- PLAN.md: short note that codegen (originally out of scope) landed and where.

## Verification

1. `cargo test` — existing 14 solver tests still pass + new codegen tests.
2. `cargo run --example codegen` — prints IR, constraints, inferred types, and C; writes `target/generated.c`; the struct def visibly contains the row-extended fields `y` and `z`.
3. `cc target/generated.c -o target/generated && ./target/generated` — compiles clean (ideally with `-Wall`) and prints `43` (via print_int) and `result: 43`.
4. `cargo run --example main` — the 7 existing examples are untouched and still behave as before.
