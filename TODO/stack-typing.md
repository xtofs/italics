# Stack typing

- **Stack representation**
  - Use `Type::Stack(Vec<Type>)` for stack shapes.
  - Add instructions for:
    - push/pop
    - call/return with stack effects.
  - Constraints:
    - `StackEqual` for matching caller/callee stack shapes.
    - integrate with `FuncType.stack`.
