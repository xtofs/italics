use std::collections::BTreeMap;

use italics::*;

fn main() {
    // Each example builds IR and optionally returns extra constraints
    // (e.g. subtyping) that no single instruction generates on its own.
    let examples: Vec<(&str, fn(&mut IRBuilder) -> Vec<Constraint>)> = vec![
        ("construct_load_call", example_construct_load_call),
        ("load_missing_field", example_load_missing_field),
        ("load_existing_field", example_load_existing_field),
        ("store_field", example_store_field),
        ("has_field_vs_field_type", example_has_field_vs_field_type),
        ("subtype_ok", example_subtype_ok),
        ("subtype_missing", example_subtype_missing),
    ];

    for (name, example_fn) in examples {
        run(name, example_fn);
    }
}

/// Construct an object { x, y }, load one of its fields, then call a function
/// with the object and the loaded value — the basic object/load/call flow.
fn example_construct_load_call(b: &mut IRBuilder) -> Vec<Constraint> {
    // Registers for fields
    let r_x = b.reg();
    let r_y = b.reg();

    // create a new object from values in the two registers: Object: { x: r_x, y: r_y }
    let obj = b.new_obj(vec![("x", r_x), ("y", r_y)]);

    // Load obj.x
    let r_z = b.load(obj, "x");

    // Call: f(obj, r_z)
    let r_f = b.reg();
    // pretend this is a function value
    let _r_ret = b.call(r_f, vec![obj, r_z]);

    vec![]
}

/// Load a field that the object was not constructed with: the open row is
/// extended through its tail to admit the missing field `y`.
fn example_load_missing_field(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let obj = b.new_obj(vec![("x", r_x)]);
    let _r_y = b.load(obj, "y"); // field "y" is added via row-tail extension
    vec![]
}

/// Load a field the object already has: the load destination simply unifies
/// with the existing field's type.
fn example_load_existing_field(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let r_y = b.reg();
    let obj = b.new_obj(vec![("x", r_x), ("y", r_y)]);
    let _r_y = b.load(obj, "y"); // field "y" exists directly
    vec![]
}

/// Store into a field the object was not constructed with. Like `load`, the
/// `store` instruction emits RowHasField/RowFieldType constraints, so the open
/// row is extended through its tail to admit `z`, and `z`'s type is unified
/// with the stored value's type.
fn example_store_field(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let obj = b.new_obj(vec![("x", r_x)]);
    let r_z = b.reg();
    b.store(obj, "z", r_z); // field "z" is added via row-tail extension
    vec![]
}

/// Highlight the division of labor between the two field constraints by
/// issuing them by hand (the `load`/`store` instructions always emit them as a
/// pair, which hides the distinction). On an object `{ x }`:
///
/// - `RowFieldType(obj, "typed_only", int)` alone — constrains a type but
///   asserts no presence, so the field is **never created**: `typed_only` does
///   not appear in the result.
/// - `RowHasField(obj, "present")` alone — asserts presence, so the open row is
///   extended to admit `present`, but its type is left an open variable.
/// - `RowHasField(obj, "both")` then `RowFieldType(obj, "both", bool)` — the
///   pair used for real field access: presence first creates the field, then
///   the type link fixes it to `bool`.
///
/// Expect the object to infer as
/// `{ x, both: bool, present: t_n | r_n }` by default (Unicode style with
/// `pretty-unicode`) — with no `typed_only`.
fn example_has_field_vs_field_type(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let obj = b.new_obj(vec![("x", r_x)]);

    vec![
        // type link with no presence assertion → vacuous, field not created
        Constraint::RowFieldType(obj.ty(), "typed_only".to_string(), Type::Int),
        // presence only → field created, type left open
        Constraint::RowHasField(obj.ty(), "present".to_string()),
        // presence + type → field created and typed. List order does not
        // matter: the solver settles all RowHasField (presence) before any
        // RowFieldType (type link), so `both` is typed `bool` regardless.
        Constraint::RowHasField(obj.ty(), "both".to_string()),
        Constraint::RowFieldType(obj.ty(), "both".to_string(), Type::Bool),
    ]
}

/// Build an interface (closed structural type) from field name/type pairs.
fn interface(fields: Vec<(&str, Type)>) -> Type {
    Type::Interface(Row {
        fields: fields
            .into_iter()
            .map(|(name, ty)| (name.to_string(), ty))
            .collect::<BTreeMap<_, _>>(),
        tail: None,
    })
}

/// Width subtyping: a two-field record { x, y } satisfies the narrower
/// interface { x: int }. The subtype check also drives inference — it forces
/// the object's `x` field (reg0) to `int`.
fn example_subtype_ok(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let r_y = b.reg();
    let point = b.new_obj(vec![("x", r_x), ("y", r_y)]);

    vec![Constraint::Subtype(
        point.ty(),
        interface(vec![("x", Type::Int)]),
    )]
}

/// The object { x } does NOT satisfy the interface { x, y }: the interface
/// requires a field `y` that the record does not provide, so the subtype
/// check reports a type error.
fn example_subtype_missing(b: &mut IRBuilder) -> Vec<Constraint> {
    let r_x = b.reg();
    let point = b.new_obj(vec![("x", r_x)]);

    vec![Constraint::Subtype(
        point.ty(),
        interface(vec![("x", Type::Int), ("y", Type::Int)]),
    )]
}

fn run(name: &str, example_fn: fn(&mut IRBuilder) -> Vec<Constraint>) {
    println!("\n=== Running {} ===", name);

    let mut builder = IRBuilder::default();

    let extra_constraints = example_fn(&mut builder);

    println!("Body:");
    for i in &builder.body {
        println!("    {}", i);
    }

    let mut solver = Solver::new(&mut builder.type_variable_generator);

    // Generate constraints from instructions, then append any extra
    // constraints the example supplied (e.g. subtyping assertions).
    let mut constraints = Vec::new();
    for instr in &builder.body {
        let mut cs = solver.generate_constraints(instr);
        constraints.append(&mut cs);
    }
    constraints.extend(extra_constraints);

    println!("Constraints:");
    for c in &constraints {
        println!("    {}", c);
    }

    if let Err(e) = solver.solve(&constraints) {
        println!("Type error: {:?}", e);
    } else {
        println!("Solved!");

        println!("    substitutions:");
        for (v, ty) in &solver.substitutions {
            println!(
                "        {} {} {}",
                v,
                italics::display::symbol(italics::display::Symbol::SubstitutionArrow),
                ty
            );
        }
    }

    println!("\n    register types:");
    for reg in builder.register_file.iter() {
        let ty = solver.apply(Type::Unknown(reg.tv));
        println!("        {}: {}", reg, ty);
    }

    if solver.substitutions.iter().any(|assoc| assoc.0.is_row()) {
        println!("\n    bound row tails:");
        for (v, ty) in &solver.substitutions {
            if v.is_row() {
                println!(
                    "        {} {} {}",
                    v,
                    italics::display::symbol(italics::display::Symbol::SubstitutionArrow),
                    ty
                );
            }
        }
    }
}
