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
  - `RowHasField(Type, String)` — row must contain a field
  - `RowFieldType(Type, String, Type)` — field type constraint
  - `Subtype(Type, Type)` — row/interface inclusion
  - `StackEqual(Vec<Type>, Vec<Type>)` — stack typing equality

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
  - `subs: HashMap<TypeVar, Type>` — substitution map from type variables to concrete types.

  Methods:
  - `resolve(var)` — get current meaning of a type variable
  - `unify(a, b)` — unify two types (currently stubbed)
  - `solve(constraints)` — iterate constraints and apply unification / specialized handling

The solver is currently a skeleton: it checks equality and reports failure otherwise. Full unification (rows, functions, existentials, stacks) is planned.

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
