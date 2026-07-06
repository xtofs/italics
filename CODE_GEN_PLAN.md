# Codegen Notes (C Backend)

This file documents C backend specifics. General architecture is in
`ARCHITECTURE.md`.

## Scope

`src/codegen.rs` lowers solved IR into a runnable C translation unit.

Key points:

- Records are emitted as heap-allocated structs (`struct Rn *` + `calloc`).
- Struct shapes are structurally deduplicated.
- Open row tails are closed at codegen time and annotated in comments.
- Function types are interned into function-pointer typedefs (`fnN`).

## Type Lowering Summary

- `Int` -> `int64_t`
- `Bool` -> `bool`
- `Ptr(t)` -> `<lowered t>*`
- `Record(row)` -> `struct Rn *`
- `Func(ft)` -> `fnN` typedef

Unsupported at codegen surface:

- `Interface`
- `Existential`
- `Stack`

Unresolved type variables are hard errors (`CodegenError::UnresolvedType`).

## Runtime Function Handling

- Known runtime names (`print_int`, `print_bool`) are emitted as static prelude functions.
- Unknown `LoadFunc` names are emitted as `extern` prototypes.

## Why This Backend Exists

It makes inference observable in generated code:

- fields introduced by row-tail extension (`load`/`store`) appear in final struct layouts
- register comments include fully resolved inferred types

## Verification Workflow

From repo root:

```bash
cargo test
cargo run --example codegen
cc target/generated.c -o target/generated
./target/generated
```

Optional Unicode display mode:

```bash
cargo run --example codegen --features pretty-unicode
```

Note: `target/generated.c` is written relative to the current working directory.
