# Display Modes

`italics` supports two display styles for symbols used in type rendering,
constraints, and debug output.

## ASCII Mode (Default)

This is the default and is convenient for plain terminals and logs.

Examples:

- type variables: `t_0`, `r_3`
- operators/symbols: `->`, `:in:`, `<:`, `=>`

## Unicode Mode

Enable Unicode output with the `pretty-unicode` feature:

```bash
cargo run --example irbuilder --features pretty-unicode
```

Unicode mode uses symbols such as:

- `τ`, `ρ`
- `→`, `∈`, `⊆`, `↦`

## Notes

- The feature affects display/formatting only.
- Core IR semantics, constraint solving, and code generation are unchanged.
