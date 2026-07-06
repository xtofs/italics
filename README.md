# italics

`italics` is a typed IR playground with row-polymorphic type inference and a C backend.

The project demonstrates:

- register-based IR construction
- constraint generation and unification
- open-row extension (`load`/`store` can grow record shape)
- code generation from inferred types to runnable C

## Quick Start

From repository root:

```bash
cargo test
cargo run --example main
cargo run --example codegen
```

The codegen example writes generated C to:

```text
target/generated.c
```

Then compile and run it manually:

```bash
cc target/generated.c -o target/generated
./target/generated
```

## Display Modes

Default output is ASCII (for logs/comments/type rendering):

- type vars: `t_0`, `r_3`
- arrows/operators: `->`, `:in:`, `<:`, `=>`

Enable Unicode prettified output:

```bash
cargo run --example main --features pretty-unicode
```

Unicode mode uses symbols like `τ`, `ρ`, `→`, `∈`, `⊆`, `↦`.

## Main Components

- IR builder and instructions: `src/builder.rs`, `src/instructions.rs`
- Types and row representation: `src/types.rs`, `src/variables.rs`
- Constraint solver: `src/solver.rs`, `src/constraints.rs`
- Display/symbol formatting: `src/display.rs`
- C code generator: `src/codegen.rs`

## Examples

- `examples/main.rs`: inference walkthrough and constraint/debug output
- `examples/codegen.rs`: end-to-end inference -> C emission -> runnable program

## Documentation

- Architecture and design reference: `ARCHITECTURE.md`
- Roadmap and upcoming work: `PLAN.md`
