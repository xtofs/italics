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
cargo run --example irbuilder
cargo run --example pipeline
cargo run --example functions
cargo run --example control
```

Examples that emit C write generated sources under `target/`.
Notable outputs include:

```text
target/generated.c
target/generated_pipeline.c
target/generated_functions.c
target/generated_control.c
```

`functions` and `control` also compile and run the generated C automatically
through `CBuild`.

Display formatting options are documented in [Display Modes](DISPLAY_MODES.md).

## Main Components

- IR builder and instructions: `src/builder.rs`, `src/instructions.rs`
- Types and row representation: `src/types.rs`, `src/variables.rs`
- Constraint solver: `src/solver.rs`, `src/constraints.rs`
- Display/symbol formatting: `src/display.rs`
- C code generator: `src/codegen.rs`

## Examples

- `examples/irbuilder.rs`: end-to-end single-function inference and C emission demo
- `examples/pipeline.rs`: stage-by-stage pipeline demo (IR -> constraints -> solving -> inferred types -> C)
- `examples/functions.rs`: program-level API demo (`IRProgram` + `emit_c_program`) with internal function calls and parameter passing
- `examples/control.rs`: structured control-flow demo (`if`/`for`) lowered to C

## Documentation

- Architecture and design reference: `ARCHITECTURE.md`
- Roadmap and upcoming work: `PLAN.md`
- Display symbol/formatting options: `DISPLAY_MODES.md`
