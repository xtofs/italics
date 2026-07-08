use italics::*;

/// Demonstrate program-level codegen with internal IR functions.
///
/// This example focuses on `Function`, `Program`, and the staged `CBuild`
/// pipeline (`generate()` → `compile()` → `run()`).
fn main() {
    let helper = build_helper();
    let entry = build_main("entry");

    let mut program = Program::new("entry");
    program.add_function(helper);
    program.add_function(entry);

    let source = CBuild::from_program("generated_functions", &program)
        .generate()
        .expect("program codegen should succeed");
    println!("\nSource generated to {}", source.source_path().display());

    let compiled = source.compile().expect("compile should succeed");
    println!("Compiled to {}", compiled.binary.display());

    let report = compiled.run().expect("run should succeed");
    println!("\nProgram output:\n{}", report.stdout);
}

fn build_helper() -> Function {
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

fn build_main(name: impl Into<String>) -> Function {
    let mut b = FunctionBuilder::new(name, [], Type::Int);

    let arg1 = b.const_int(10);
    let arg2 = b.const_int(33);
    let helper_fn = b.func("helper", vec![Type::Int, Type::Int], Type::Int);
    let value = b.call(helper_fn, vec![arg1, arg2]);

    let print_int_fn = b.prelude("print_int");
    let _ = b.call(print_int_fn, vec![value]);

    b.ret(value);

    b.build()
}
