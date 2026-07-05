use italics::*;

fn main() {
    let examples: Vec<(&str, fn(&mut IRBuilder))> = vec![
        ("example0", example0),
        ("example1", example1),
        ("example2", example2),
    ];

    for (name, example_fn) in examples {
        run(name, example_fn);
    }
}

fn example0(b: &mut IRBuilder) {
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
}

fn example1(b: &mut IRBuilder) {
    let r_x = b.reg();
    let obj = b.new_obj(vec![("x", r_x)]);
    let _r_y = b.load(obj, "y"); // field "y" does NOT exist
}

fn example2(b: &mut IRBuilder) {
    let r_x = b.reg();
    let obj = b.new_obj(vec![("x", r_x)]);
    let _r_y = b.load(obj, "y"); // field "y" does NOT exist
}

fn run(name: &str, example_fn: fn(&mut IRBuilder)) {
    println!("\n=== Running {} ===", name);

    let mut builder = IRBuilder::new();

    example_fn(&mut builder);

    println!("Body:");
    for i in &builder.body {
        println!("    {}", i);
    }

    let mut solver = Solver::new(&mut builder.type_variable_generator);

    // Generate constraints
    let mut constraints = Vec::new();
    for instr in &builder.body {
        let mut cs = solver.generate_constraints(instr);
        constraints.append(&mut cs);
    }

    println!("Constraints:");
    for c in &constraints {
        println!("    {}", c);
    }

    if let Err(e) = solver.solve(&constraints) {
        println!("Type error: {:?}", e);
    } else {
        println!("Solved!");

        for (v, ty) in &solver.subs {
            println!("    {} ↦ {}", v, ty);
        }

        println!("Resolved register types:");
        for reg in builder.register_file.iter() {
            let ty = solver.resolve_type(Type::Unknown(reg.ty));
            println!("        {}: {}", reg, ty);
        }
    }
}
