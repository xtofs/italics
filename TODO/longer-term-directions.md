# Longer-term directions

- **Row calculus alignment**
  - Align row polymorphism with formal row calculi (Rémy, MLPolyR).
- **iTalX-style global inference**
  - Extend from per-function to whole-program inference if needed.
- **Wasm / LLVM integration**
  - Explore emitting typed Wasm or richer LLVM IR with metadata.

This plan takes the current skeleton to a usable, TAL/iTalX-inspired typed compiler that can infer types, model objects and interfaces structurally, and eventually emit C or LLVM.
