# Typed IR Architecture (Rust, TAL/iTalX-inspired)

## 1. Core concepts

- **Type variables (`TypeVar`)**  
  Immutable identifiers representing unknown types. They are generated per function via `TypeVarGenerator` and never mutated. The solver owns their meaning.

- **Types (`Type`)**  
  A small type algebra supporting:
  - `Int`, `Bool`
  - `Ptr(Box<Type>)`
  - `Func(FuncType)` — parameter list, return type, optional stack typing
  - `Record(Row)` — structural object types
  - `Interface(Row)` — structural interface types (row-polymorphic)
  - `Existential(Existential)` — existential packages hiding a type variable
  - `Stack(Vec<Type>)` — stack typing
  - `Unknown(TypeVar)` — reference to a type variable

- **Rows (`Row`)**  
  Structural records/interfaces with:
  - `fields: BTreeMap<String, Type>`
  - `tail: Option<TypeVar>` — row polymorphism tail (open rows)

- **Registers (`Reg`)**  
  Per-function virtual registers:
  - `id: RegId`
  - `ty: TypeVar` — handle into the type system

  Registers never store concrete types, only type variables.

- **RegisterFile + RegGenerator**  
  Per-function register storage and allocation. Each function has its own `RegGenerator` and `RegisterFile`.

- **IR Builder (`IRBuilder`)**  
  Ergonomic API to construct typed IR:
  - `reg()` — allocate a new register with a fresh type variable
  - `new_obj(fields)` — construct a `NewObj` instruction and return destination register
  - `load(src, field)` — `Load` instruction, returning destination register
  - `store(dst, field, src)` — `Store` instruction
  - `call(func, args)` — `Call` instruction, returning destination register

- **Instructions (`Instr`)**  
  Current set:
  - `Load { dst, src, field }`
  - `Store { dst, field, src }`
  - `NewObj { dst, fields }`
  - `Call { func, args, ret }`

## 2. Constraint system

- **Constraints (`Constraint`)**  
  Represent relationships between types:
  - `Equal(Type, Type)` — unification
  - `RowHasField(Type, String)` — the row **has** a field of the given name (presence/shape only, printed `f ∈ r`)
  - `RowFieldType(Type, String, Type)` — the named field's **type** must unify with the given type; it does not itself require the field to exist (printed `f: t ∈ r`)
  - `Subtype(Type, Type)` — row/interface inclusion
  - `StackEqual(Vec<Type>, Vec<Type>)` — stack typing equality

- **`RowHasField` vs `RowFieldType`**  
  These are the two complementary halves of "row access", split by
  responsibility:
  - `RowHasField(row, f)` owns **existence**: it asserts that `f` is present.
    The solver (`check_row_has_field`) looks the field up and, if it is
    missing on an open row, extends the row's tail to admit it (a missing
    field on a closed row is an error). It constrains the record's *shape*,
    not the field's type.
  - `RowFieldType(row, f, t)` owns the **type link**: it asserts that `f`'s
    type unifies with `t`. It does not, on its own, require `f` to exist —
    that is `RowHasField`'s job.

  The two are emitted together as a pair for `Load` and `Store`, so
  `RowHasField` guarantees the field is there (extending the row if needed)
  and `RowFieldType` ties its type to a register. Example: `load dst = obj.f`
  emits `f ∈ τ_obj` (obj must have `f`) and `f: τ_dst ∈ τ_obj` (that `f`'s
  type is the load destination's type).

- **Constraint generation (`generate_constraints`)**  
  For each instruction:
  - `Load`:
    - `RowHasField(src.ty(), field)`
    - `RowFieldType(src.ty(), field, dst.ty())`
  - `Store`:
    - `RowHasField(dst.ty(), field)`
    - `RowFieldType(dst.ty(), field, src.ty())`
  - `NewObj`:
    - Build a `Row` with field types from registers and a fresh tail `TypeVar`
    - `Equal(dst.ty(), Record(row))`
  - `Call`:
    - Construct a `FuncType` from argument register types and return register type
    - `Equal(func.ty(), Func(func_type))`

## 3. Solver

- **Solver (`Solver`)**  
  Holds:
  - `substitutions: HashMap<TypeVar, Type>` — substitution map from type variables to types.
  - `tvg: &mut TypeVarGenerator` — the same generator the IRBuilder uses, so solver-created variables never collide.
  - `row_tail_vars: HashSet<TypeVar>` — every row-tail variable the solver created.

- **Row-kind variables**  
  Kinds are encoded in the `TypeVar` itself (`vars.rs`): `fresh_row()` sets a tag bit, `TypeVar::is_row()` tests it. `bind_var` kind-checks every binding: a row variable may only be bound to a row fragment (a `Record`) or another row variable (`TypeError::KindMismatch` otherwise).

  Methods:
  - `resolve(var)` — get current meaning of a type variable
  - `resolve_type(ty)` — chase substitutions; flattens row types via `resolve_row`
  - `resolve_row(row)` — merge in the fields of every row fragment bound along the tail chain
  - `apply(ty)` — deep substitution for reporting final inferred types
  - `unify(a, b)` — full unification: vars (with occurs check), `Ptr`, `Func`, `Record`/`Interface` rows, `Stack`
  - `unify_row(r1, r2)` — Rémy-style open-row unification: shared fields unify pointwise, exclusive fields are absorbed into the other side's tail, both-open rows share a fresh common tail
  - `solve(constraints)` — iterate constraints and apply unification / specialized handling
  - `lookup_or_extend_field(ty, name)` — backs `RowHasField`/`RowFieldType`: returns the field's type, extending an open row via its tail when the field is missing (row-tail extension)

Not yet implemented: existential unification, `Subtype` beyond `Record <: Interface`.

## 4. Example flow

1. Create `IRBuilder`.
2. Allocate registers via `reg()`.
3. Build instructions (`new_obj`, `load`, `call`).
4. Iterate over `body` and call `generate_constraints` for each instruction.
5. Collect constraints.
6. Pass constraints to `Solver::solve`.
7. Later: use solved types to emit C or LLVM IR.

This architecture mirrors iTalX/TAL ideas:
- per-function type variables and registers
- structural object/interface typing via rows
- constraint-based type reconstruction
- separation between IR and solver (IR never mutated).
