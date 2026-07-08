use std::fs;
use std::path::Path;

use italics::instructions::BinOpKind;
use italics::*;

/// Demonstrate the single-function pipeline by printing every stage:
///
/// 1) IR body
/// 2) generated constraints
/// 3) solved substitutions
/// 4) inferred register types
/// 5) emitted C
fn main() {
    println!("== Stage 1: Build IR ==");
    let mut b = InstructionBuilder::default();

    let x = b.const_int(21);
    let y = b.const_int(21);
    let sum = b.binop(BinOpKind::Add, x, y);
    let obj = b.new_obj(vec![("sum", sum)]);
    let loaded = b.load(obj, "sum");
    let f = b.prelude("print_int");
    let _ = b.call(f, vec![loaded]);
    b.ret(loaded);

    for instr in &b.body {
        println!("    {}", instr);
    }

    println!("\n== Stage 2: Generate Constraints ==");
    let body = b.body.clone();
    let constraints = Inference::new(&body, &b.register_file)
        .generate_constraints(&mut b.type_variable_generator);
    for c in &constraints.constraints {
        println!("    {}", c);
    }

    println!("\n== Stage 3: Solve Constraints ==");
    let solved = constraints
        .solve(&mut b.type_variable_generator)
        .expect("pipeline example should solve");
    for (v, ty) in &solved.solver.substitutions {
        println!(
            "    {} {} {}",
            v,
            italics::display::symbol(italics::display::Symbol::SubstitutionArrow),
            ty
        );
    }

    println!("\n== Stage 4: Inferred Register Types ==");
    for reg in b.register_file.iter() {
        println!("    {}: {}", reg, solved.solver.apply(reg.ty()));
    }

    println!("\n== Stage 5: Emit C ==\n");
    let c = solved.generate_code().expect("codegen should succeed");
    println!("{}", c);

    let target = Path::new("target");
    if target.is_dir() {
        let out = target.join("generated_pipeline.c");
        fs::write(&out, &c).expect("write generated_pipeline.c");
        println!("(written to {})", out.display());
    }
}
