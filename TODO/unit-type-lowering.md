# Revisit: `Unit` lowering — singleton `unit_t` vs C `void`

**Status:** open / deferred. Current code keeps `unit_t`. This note records the
tradeoff so the decision can be revisited deliberately, not by accident.

## Current design

`Type::Unit` is a first-class **singleton value type** lowered to a one-byte C
struct:

```c
typedef struct { char _; } unit_t;          // emitted in the preamble
static unit_t print_int(int64_t x) { printf("%lld\n", (long long)x); return (unit_t){0}; }
```

A unit-returning call whose result is unused emits the bare statement (the normal
case), so the struct is never materialized there:

```c
reg4(reg3);   // call reg5 := reg4(reg3), reg5 : unit — result dropped
```

## The observation that prompted this

`unit_t` currently shows up in three spots (the call decl is *not* one of them —
the dropped result is already handled by the bare `reg4(reg3);`):

1. the fn-pointer typedef: `typedef unit_t (*fn0)(int64_t);`
2. the prelude body: `return (unit_t){0};`
3. the prelude declaration: `static unit_t print_int(int64_t x)`

Since nothing ever *keeps* a unit value today, we could instead lower `Unit → void`:

```c
static void print_int(int64_t x) { printf("%lld\n", (long long)x); }
// call site unchanged:
print_int(reg3);
```

...and delete the `unit_t` struct entirely.

## Why we did NOT switch to `void`

C's `void` is **not a value type** — you can't declare, store, pass, or return it.
`unit_t` (a real one-value type) can. The following lower with **zero special
cases** under `unit_t` and become **illegal C** under `void`:

| situation | `unit_t` | `void` |
|---|---|---|
| a **used** call result (`let r = call(print_int,[x]); …r…`) | `unit_t r = print_int(x);` ✓ | `void r = …;` ✗ |
| a unit **struct field** (`new { u: someUnit }`) | `unit_t u;` ✓ | `void u;` ✗ |
| a unit **parameter** (`f : (unit) -> int`) | `int f(unit_t x)` ✓ | `f(void x)` ✗ |
| **returning** unit from an IR function | `return u;` ✓ | `return;` (+ can't name the value) |

Under `unit_t`, unit is "a value like any other" and the register/codegen model
stays uniform — no branches anywhere. Under `void`, unit is "the absence of a
value," so every place it could be materialized needs an "if void, skip/error"
branch, **and** you need a guard (the `ground_registers` unit-check that was
prototyped then deleted) to *prohibit* keeping a unit value — otherwise you
silently emit broken C the moment someone does.

None of the four rows above is exercised today (unit only ever appears as an
immediately-dropped prelude call result), so `void` genuinely works *right now* —
it's a YAGNI-vs-fidelity call, not a correctness one.

## Recommendation / when to revisit

- **Keep `unit_t`** while unit stays first-class and we care about type fidelity
  (this is a type-inference project; "unit = a type with one value" is honest, and
  `unit_t` is its faithful lowering at the cost of a single warning-free typedef).
- **Switch to `void`** only if we deliberately commit to "unit is always dropped"
  and prefer idiomatic C output. If we do, the work is:
  - `lower`/`lower_readonly`: `Type::Unit => "void"`.
  - prelude `c_def`s: `static void … { … }` (no `return`); drop the `unit_t`
    typedef from the preamble.
  - `Call` emit arm: **never** bind a void-returning result (currently binds when
    `used`).
  - re-add a `ground_registers` guard: a register whose type is `Unit`/void is a
    `CodegenError` (enforces "unit never escapes").

## Related follow-ups (also deferred)

- `const_unit` / a way to construct a unit **value** in the IR.
- User-defined unit-returning `IRFunction`s (would also want
  `Ret { src: Option<Reg> }` and void entry-wrapper handling).
