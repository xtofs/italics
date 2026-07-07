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

    // sum = for i in 0..10, acc = 0 { acc + i }   => 45
    let ten = b.const_int(10);
    let zero = b.const_int(0);
    let sum = b.for_acc(ten, zero, |b, index, acc| {
        b.binop(BinOpKind::Add, acc, index)
    });

    // pick = if true { 1 } else { 2 }             => 1
    let cond = b.const_bool(true);
    let pick = b.if_value(cond, |b| b.const_int(1), |b| b.const_int(2));

    // total = sum + pick                           => 46
    let total = b.binop(BinOpKind::Add, sum, pick);
    let print_int = b.prelude("print_int");
    let _ = b.call(print_int, vec![total]);
    b.ret(total);

    let build = CBuild::from_builder("generated_control", b).expect("program should type-check");

    let report = build.run().expect("compile and run should succeed");

    println!("source generated to {}", build.source_path().display());
    println!("compiled to {}", build.binary_path().display());

    println!("Program output:\n{}", report.stdout);
}
