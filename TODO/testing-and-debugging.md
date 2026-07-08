# Testing and debugging

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
