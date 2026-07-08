# Row polymorphism and interfaces

- **Row unification**
  - Implement support for:
    - closed rows (no tail)
    - open rows (tail `Some(TypeVar)`)
  - `Subtype(Type::Record, Type::Interface)`:
    - record row must contain at least the fields of the interface row.
    - unify field types accordingly.
  - `RowHasField` and `RowFieldType`:
    - integrate with row unification:
      - if field missing, either:
        - add it to an open row via tail variable, or
        - report type error for closed rows.

- **Interface modeling**
  - Decide whether `Interface(Row)` is:
    - purely structural (current design), or
    - later extended with nominal IDs.
  - Add constraints for interface satisfaction:
    - `Subtype(Record(row_obj), Interface(row_iface))`.
