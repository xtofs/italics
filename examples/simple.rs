use std::fs;
use std::path::Path;

use italics::instructions::BinOpKind;
use italics::*;

/// End-to-end demonstration: build a small IR program, infer its types, and
/// emit runnable C. The headline observable is that the generated `struct R0`
/// contains the fields `y` and `z` — neither of which appears in the `new { x }`
/// literal. They are added purely by inference (row-tail extension) from the
/// later `store`/`load` instructions, and show up in the physical struct layout.
fn main() {
    let mut b = InstructionBuilder::default();

    // n = 42
    let n = b.const_int(42);
    // obj = { x: n }            — struct starts as { x }
    let obj = b.new_obj(vec![("x", n)]);
    // store obj.y = n           — row extension: struct R0 gains `y`
    b.store(obj, "y", n);
    // y = load obj.y            — reads the extension-created field back
    let y = b.load(obj, "y");
    // one = 1
    let one = b.const_int(6);
    // sum = y + one             — forces y : int; sum = 43
    let sum = b.binop(BinOpKind::Add, y, one);
    // f = @print_int : (int) → int   — runtime fn, signature as constraint
    let f = b.prelude("print_int");
    // call f(sum)               — prints 43
    let _ = b.call(f, vec![sum]);
    // store obj.z = sum         — row extension again: struct R0 gains `z`
    b.store(obj, "z", sum);
    // ret sum
    b.ret(sum);

    println!("Body:");
    for i in &b.body {
        println!("    {}", i);
    }

    let body = b.body.clone();

    let constraints = Inference::new(&body, &b.register_file)
        .generate_constraints(&mut b.type_variable_generator);
    println!("\nConstraints:");
    for c in &constraints.constraints {
        println!("    {}", c);
    }

    let solved = constraints
        .solve(&mut b.type_variable_generator)
        .expect("program should type-check");

    println!("\nInferred register types:");
    for reg in b.register_file.iter() {
        println!("    {}: {}", reg, solved.solver.apply(reg.ty()));
    }

    let c = solved.generate_code().expect("codegen should succeed");

    println!("\nGenerated C:\n");
    println!("{}", c);

    // Write it out so the verification step can compile and run it.
    let target = Path::new("target");
    if target.is_dir() {
        let out = target.join("generated.c");
        fs::write(&out, &c).expect("write generated.c");
        println!("(written to {})", out.display());
    }
}
