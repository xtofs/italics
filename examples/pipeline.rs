use std::cmp::max;
use std::path::Path;
use std::{fmt, fs};

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
    // //////////////////////////////////////////////
    print_stage_header("== Stage 1: Build IR ==");

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

    // /////////////////////////////////////////////////////////
    print_stage_header("== Stage 2: Generate Constraints ==");

    let body = b.body.clone();
    let constraints = Inference::new(&body, &b.register_file).generate_constraints(&mut b.type_variable_generator);
    for c in &constraints.constraints {
        println!("    {}", c);
    }

    // ////////////////////////////////////////////////////////
    print_stage_header("== Stage 3: Solve Constraints ==");

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

    // ////////////////////////////////////////////
    print_stage_header("== Stage 4: Inferred Register Types ==");

    for reg in b.register_file.iter() {
        println!("    {}: {}", reg, solved.solver.apply(reg.ty()));
    }

    // ////////////////////////////////////////////
    print_stage_header("== Stage 5: Emit C ==");
    let c = solved.generate_code().expect("codegen should succeed");
    println!("{}", c);

    let target = Path::new("target");
    if target.is_dir() {
        let out = target.join("generated_pipeline.c");
        fs::write(&out, &c).expect("write generated_pipeline.c");
        println!("(written to {})", out.display());
    }
}

fn print_stage_header(header: &str) {
    const WIDTH: usize = 80;
    let n = max(header.len(), WIDTH);
    println!();
    println!("{}", Chars(b'=', n));
    println!("{:=^width$}", header, width = WIDTH);
    println!("{}", Chars(b'=', n));
    println!();
}

struct Chars(u8, usize);

impl fmt::Display for Chars {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let buffer: [u8; 64] = [self.0; 64]; // max chunk
        let mut remaining = self.1;

        while remaining > 0 {
            let chunk = remaining.min(buffer.len());
            fmt.write_str(std::str::from_utf8(&buffer[..chunk]).unwrap())?;
            remaining -= chunk;
        }

        Ok(())
    }
}
