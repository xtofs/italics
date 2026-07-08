# IR extensions

- **More instructions**
  - Arithmetic: `Add`, `Sub`, `Mul`, `Div`:
    - constraints: operands and result must be `Int` (or numeric types later).
  - Control flow:
    - `Branch`, `Jump`, `Phi` (if SSA):
      - constraints for merging types at join points.
  - Memory:
    - `LoadPtr`, `StorePtr` for `Ptr` types.

- **SSA support (optional)**
  - Extend `RegId` to encode SSA versions.
  - Add builder helpers for phi nodes.
