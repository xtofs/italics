# Existential types

- **Existential packages**
  - Add instructions for:
    - `pack` — create existential package from a concrete type and value.
    - `unpack` — open existential package with a fresh type variable.
  - Constraint generation:
    - `Existential(Existential)` must hide the internal `TypeVar`.
    - Unification must respect existential boundaries.
