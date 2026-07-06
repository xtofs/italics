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
    let entry = build_entry("entry");

    let mut program = IRProgram::new("entry");
    program.add_function(helper);
    program.add_function(entry);

    let c = emit_c_program(&program).expect("program codegen should succeed");

    println!("Generated C for program with internal calls:\n");
    let target = Path::new("./target");
    if target.is_dir() {
        let c_file = target.join("generated.c");
        let binary_file = target.join("generated");
        fs::write(&c_file, &c).expect(&format!(
            "write {}",
            c_file.to_str().expect("file path not a string")
        ));
        println!("written to {}", c_file.display());
        println!("");

        let _ = run_cmd!(
            "cc",
            "-Wall",
            "-O2", // should be at least -O1 to allow for allocation optimization
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
    // fn hello(a,b) -> a * 7 + b * 2
    let mut b = FunctionBuilder::new("helper", [Type::Int, Type::Int], Type::Int);

    let [input1, input2] = b.params(); // note that the amount of parameters has to corrspond to the parameter type array, otherwise we get a compile time error

    let seven = b.const_int(7);
    let partial1 = b.binop(italics::instructions::BinOpKind::Mul, input1, seven);

    let two = b.const_int(2);
    let partial2 = b.binop(italics::instructions::BinOpKind::Mul, input2, two);

    let result = b.binop(italics::instructions::BinOpKind::Add, partial1, partial2);
    b.ret(result);

    b.build()
}

fn build_entry(name: impl Into<String>) -> IRFunction {
    let mut b = FunctionBuilder::new(name, [], Type::Int);

    let arg1 = b.const_int(10);
    let arg2 = b.const_int(33);
    let helper_fn = b.func("helper", vec![Type::Int, Type::Int], Type::Int);
    let value = b.call(helper_fn, vec![arg1, arg2]);

    let print_int_fn = b.func("print_int", vec![Type::Int], Type::Int);
    let _ = b.call(print_int_fn, vec![value]);

    b.ret(value);

    b.build()
}
