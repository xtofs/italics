# Backend: C / LLVM

- **Type mapping**
  - Map `Type` to C/LLVM types:
    - `Int` → `int` / `i32`
    - `Bool` → `_Bool` / `i1`
    - `Ptr(t)` → `t*` / `ptr`
    - `Record(Row)` → `struct` / `%struct`
    - `Func(FuncType)` → function signatures.

- **Code generation**
  - For each function:
    - use solved types from `Solver` to:
      - emit struct definitions for records.
      - emit function prototypes.
      - emit function bodies using registers as temporaries.
