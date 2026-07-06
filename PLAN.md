# Project Plan

This file tracks current status and near-term work. Architectural detail lives
in `ARCHITECTURE.md`. C backend specifics live in `CODE_GEN_PLAN.md`.

## Current Status

Completed:

- Row-polymorphic inference with open-row extension.
- Weighted constraint solving (`RowHasField` -> `RowFieldType` -> `Equal` -> relational).
- C backend (`emit_c`) with structural record deduplication.
- End-to-end examples for inference and codegen.
- Display centralization in `src/display.rs` with ASCII default and Unicode feature mode.

## Active Goals

1. Keep docs concise and non-duplicated.
2. Preserve deterministic outputs in examples and generated code.
3. Maintain strong failure behavior for unresolved/unsupported types.

## Next Milestones

1. Expand subtype support beyond `Record <: Interface`.
2. Add existential type solving (or formally scope it out).
3. Improve codegen diagnostics with richer source/instruction context.
4. Add a small CLI wrapper for stable generation workflows.

## Non-Goals (for now)

- General fixpoint constraint solving.
- LLVM backend implementation.
- Runtime GC / ownership model beyond current `calloc`-based demo output.
