use italics::instructions::BinOpKind;
use italics::*;

/// Structured control-flow codegen, end to end.
///
///  - `for i in 0..10, acc = 0 { acc + i }` infers `acc : int` from the checked
///    loop invariant `acc = next` and lowers to a C `for` with a hoisted
///    accumulator (sum = 45).
///  - `if true { 1 } else { 2 }` merges to an `int` and lowers to a hoisted
///    variable assigned in both branches (pick = 1).
///
/// The constraint-generation / solve pipeline is covered by the other examples;
/// here `CBuild::from_builder` runs it internally.
fn main() {
    let mut b = InstructionBuilder::default();

    let print_int = b.prelude("print_int");

    // sum = for i in 0..10, acc = 0 { acc + i }   => 45
    let ten = b.const_int(10);
    let zero = b.const_int(0);
    let sum = b.for_acc(ten, zero, |b, index, acc| {
        let reg = b.binop(BinOpKind::Add, acc, index);
        let _ = b.call(print_int, vec![acc]);
        reg
    });

    // pick = if true { 1 } else { 2 }             => 1
    let cond = b.const_bool(true);
    let pick = b.if_value(cond, |b| b.const_int(1), |b| b.const_int(2));

    // total = sum + pick                           => 46
    let total = b.binop(BinOpKind::Add, sum, pick);
    let _ = b.call(print_int, vec![total]);
    b.ret(total);

    for i in b.body.iter() {
        println!("{}", i);
    }

    let source = CBuild::from_builder("generated_control", b)
        .expect("program should type-check")
        .generate()
        .expect("codegen should succeed");
    println!("source generated to {}", source.source_path().display());

    let compiled = source.compile().expect("compile should succeed");
    println!("compiled to {}", compiled.binary.display());

    let report = compiled.run().expect("run should succeed");
    println!("Program output:\n{}", report.stdout);
}
