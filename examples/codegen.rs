use std::fs;
use std::path::Path;

use italics::*;

#[macro_use]
#[path = "codegen/cmd.rs"]
mod cmd;

/// Demonstrate program-level codegen with internal IR functions.
///
/// This example focuses on `IRFunction`, `IRProgram`, and `emit_c_program`.
fn main() {
    let helper = build_helper();
    let entry = build_entry();

    let mut program = IRProgram::new("entry");
    program.add_function(helper);
    program.add_function(entry);

    let c = emit_c_program(&program).expect("program codegen should succeed");

    println!("Generated C for program with internal calls:\n");
    let target = Path::new("target");
    if target.is_dir() {
        let c_file = target.join("generated_program.c");
        let binary_file = target.join("generated_program");
        fs::write(&c_file, &c).expect("write generated_program.c");
        println!("written to {}", c_file.display());
        println!("");

        let _ = run_cmd!(
            "cc",
            "-Wall",
            c_file.into_os_string(),
            "-o",
            binary_file.clone().into_os_string(),
        )
        .expect("failed to execute process");

        let _ = run_cmd!(binary_file.as_os_str()).expect("failed to execute process");
    } else {
        println!("{}", c);
    }
}

fn build_helper() -> IRFunction {
    let mut b = IRBuilder::default();

    // Reserve and use the first parameter register (reg0).
    let input = b.param(0);
    let forty = b.const_int(40);
    let two = b.const_int(2);
    let partial = b.binop(italics::instructions::BinOpKind::Add, input, forty);
    let result = b.binop(italics::instructions::BinOpKind::Add, partial, two);
    b.ret(result);

    b.finish("helper", vec![Type::Int], Type::Int)
}

fn build_entry() -> IRFunction {
    let mut b = IRBuilder::default();

    let arg = b.const_int(10);
    let helper_fn = b.func("helper", vec![Type::Int], Type::Int);
    let value = b.call(helper_fn, vec![arg]);

    let print_int_fn = b.func("print_int", vec![Type::Int], Type::Int);
    let _ = b.call(print_int_fn, vec![value]);

    b.ret(value);

    b.finish("entry", vec![], Type::Int)
}
