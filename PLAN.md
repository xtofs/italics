# Project Plan

This file is the single planning document for upcoming work.

Architecture and implemented behavior are documented in `ARCHITECTURE.md` and
`README.md`.

## Landed

- **Code generation** (originally out of scope): a self-contained C backend
  (`src/codegen.rs`, `emit_c` / `emit_c_program`) lowering inferred types to
  runnable C. See ARCHITECTURE.md §7.
- **Structured control flow**: value-producing `If`/`For` combinator blocks
  (`examples/control.rs`). Loops use the *checked* invariant `Equal(acc, next)`
  rather than fixpoint inference, keeping the solver linear. See
  ARCHITECTURE.md §3.1. Next step: loosen the "no `Ret` inside a block" and
  "block-local registers don't escape" restrictions if a surface language needs
  them.

## Active Goals

1. Keep docs concise and avoid duplication.
2. Keep generated outputs deterministic and easy to diff.
3. Preserve strict error behavior for unresolved/unsupported type lowering.

## Milestones

1. Type system and solver
   - Expand subtype handling beyond `Record <: Interface`.
   - Add existential type solving or explicitly remove existentials from surface APIs.
   - Improve error reporting with clearer source context.

2. Code generation ergonomics
   - Add richer diagnostics in `CodegenError` (instruction/register context).
   - Make `examples/codegen.rs` output path independent from invocation directory.
   - Add small utility entry point for stable `emit_c` workflows.

3. Testing and validation
   - Add targeted tests for subtype edge cases and existential behavior decisions.
   - Add codegen golden tests for display modes (ASCII and `pretty-unicode`).
   - Expand integration verification for generated C compile/run behavior.

## Non-Goals (for now)

- General fixpoint constraint solving.
- LLVM backend implementation.
- Runtime GC/ownership model beyond current `calloc`-based demo semantics.
