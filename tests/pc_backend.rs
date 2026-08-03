mod common;

use std::fs;

use sane::interpreter::Program as BfProgram;
use sane::{bf, ir, lexer, parser, sema};

use common::run_bf;

fn resolve(src: &str) -> sema::ResolvedProgram {
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    sema::resolve(&program).unwrap()
}

fn compile_pc(src: &str) -> String {
    bf::compile_pc(&resolve(src)).unwrap()
}

fn assert_stats_within(actual: ir::IrStats, maximum: ir::IrStats) {
    assert!(actual.blocks <= maximum.blocks, "{actual:?}");
    assert!(
        actual.reachable_blocks <= maximum.reachable_blocks,
        "{actual:?}"
    );
    assert!(actual.ops <= maximum.ops, "{actual:?}");
    assert!(actual.empty_blocks <= maximum.empty_blocks, "{actual:?}");
    assert!(actual.trivial_jumps <= maximum.trivial_jumps, "{actual:?}");
    assert!(actual.calls <= maximum.calls, "{actual:?}");
    assert!(actual.branches <= maximum.branches, "{actual:?}");
}

#[test]
fn ir_stats_classify_control_flow_and_reachability() {
    let program = ir::Program {
        blocks: vec![
            ir::Block {
                ops: vec![ir::Op::Puts(b"entry".to_vec())],
                terminator: ir::Terminator::Branch {
                    cond: sema::ResolvedExpr::Byte(1),
                    then_target: ir::BlockId(1),
                    else_target: ir::BlockId(2),
                },
            },
            ir::Block {
                ops: Vec::new(),
                terminator: ir::Terminator::Call {
                    function: 0,
                    args: Vec::new(),
                    return_target: ir::BlockId(3),
                },
            },
            ir::Block {
                ops: Vec::new(),
                terminator: ir::Terminator::Jump(ir::BlockId(3)),
            },
            ir::Block {
                ops: vec![ir::Op::Puts(b"done".to_vec())],
                terminator: ir::Terminator::Halt,
            },
            ir::Block {
                ops: Vec::new(),
                terminator: ir::Terminator::ReturnValue,
            },
            ir::Block {
                ops: Vec::new(),
                terminator: ir::Terminator::Jump(ir::BlockId(5)),
            },
        ],
        entry: ir::BlockId(0),
        function_entries: vec![ir::BlockId(4)],
    };

    assert_eq!(
        program.stats(),
        ir::IrStats {
            blocks: 6,
            reachable_blocks: 5,
            ops: 2,
            empty_blocks: 4,
            trivial_jumps: 2,
            calls: 1,
            branches: 1,
        }
    );
}

#[test]
fn superblocks_join_only_unique_predecessor_jump_chains() {
    let block = |terminator| ir::Block {
        ops: Vec::new(),
        terminator,
    };
    let program = ir::Program {
        blocks: vec![
            block(ir::Terminator::Jump(ir::BlockId(1))),
            block(ir::Terminator::Jump(ir::BlockId(2))),
            block(ir::Terminator::Branch {
                cond: sema::ResolvedExpr::Byte(1),
                then_target: ir::BlockId(3),
                else_target: ir::BlockId(4),
            }),
            block(ir::Terminator::Jump(ir::BlockId(5))),
            block(ir::Terminator::Jump(ir::BlockId(5))),
            block(ir::Terminator::Halt),
            block(ir::Terminator::Jump(ir::BlockId(7))),
            block(ir::Terminator::ReturnValue),
        ],
        entry: ir::BlockId(0),
        function_entries: vec![ir::BlockId(6)],
    };

    assert_eq!(
        program.superblocks(),
        vec![
            ir::Superblock {
                blocks: vec![ir::BlockId(0), ir::BlockId(1), ir::BlockId(2)],
            },
            ir::Superblock {
                blocks: vec![ir::BlockId(3)],
            },
            ir::Superblock {
                blocks: vec![ir::BlockId(4)],
            },
            ir::Superblock {
                blocks: vec![ir::BlockId(5)],
            },
            ir::Superblock {
                blocks: vec![ir::BlockId(6), ir::BlockId(7)],
            },
        ]
    );
}

#[test]
fn pc_backend_runs_linear_programs() {
    let bf = compile_pc("let x = 'A'; put x; puts \"\\n\";");
    assert_eq!(run_bf(&bf, &[]), b"A\n");
}

#[test]
fn pc_backend_runs_int_scalar_storage_and_casts() {
    let bf = compile_pc(
        "let value: int = 0x12345641; let copy: int = value; value = 0xabcdef42; put copy as byte; put value as byte;",
    );
    assert_eq!(run_bf(&bf, &[]), b"AB");
}

#[test]
fn pc_backend_runs_int_add_sub_and_equality() {
    let bf = compile_pc(
        "let value: int = 65535; value = value + 1; if value == 65536 { put 'A'; } value = value - 2; if value == 65534 { put 'B'; }",
    );
    assert_eq!(run_bf(&bf, &[]), b"AB");
}

#[test]
fn pc_backend_runs_int_bitwise_and_signed_ordering() {
    let bf = compile_pc(
        "let x: int = -1; let y: int = 0x0f0f0f0f; if x < 0 { put 'A'; } if (x & y) == y { put 'B'; } if 0x80000000 < x { put 'C'; }",
    );
    assert_eq!(run_bf(&bf, &[]), b"ABC");
}

#[test]
fn pc_backend_runs_int_multiplication() {
    let bf = compile_pc(
        "let left: int = -25; let right: int = 40; let product: int = left * right; if product == -1000 { put 'A'; }",
    );
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn pc_backend_runs_int_division_and_remainder() {
    let bf = compile_pc(
        "let left: int = -1000; let right: int = 33; if left / right == -30 { put 'A'; } if left % right == -10 { put 'B'; }",
    );
    assert_eq!(run_bf(&bf, &[]), b"AB");
}

#[test]
fn pc_backend_runs_int_decimal_io() {
    let bf = compile_pc(
        "let value: int; read value; print \"value: {} next: {:d}\\n\", value, value + 1;",
    );
    assert_eq!(
        run_bf(&bf, b"-1000000000\n"),
        b"value: -1000000000 next: -999999999\n"
    );
}

#[test]
fn profiled_pc_backend_counts_dispatches_without_changing_output() {
    let program = resolve("put 'A';");
    let bf = bf::compile_pc_with_profiling(&program).unwrap();
    let parsed = BfProgram::parse(&bf).unwrap();
    let mut output = Vec::new();
    let stats = parsed.run_with_stats(&[][..], &mut output).unwrap();

    assert_eq!(output, b"A");
    assert_eq!(stats.dispatches, 1);
}

#[test]
fn pc_backend_runs_if_else() {
    let src = "\
let x = 3;
if x == 3 {
  puts \"yes\";
} else {
  puts \"no\";
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"yes");
}

#[test]
fn pc_backend_runs_while_loops() {
    let src = "\
let x = 3;
while x {
  put '0' + x;
  x -= 1;
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"321");
}

#[test]
fn pc_backend_runs_break_and_continue() {
    let src = "\
let x = 0;
loop {
  x += 1;
  if x == 2 {
    continue;
  }
  if x == 5 {
    break;
  }
  put '0' + x;
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"134");
}

#[test]
fn pc_backend_runs_for_loops() {
    let src = "\
for let i = 0; i < 4; i += 1 {
  put 'A' + i;
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"ABCD");
}

#[test]
fn pc_backend_runs_arrays_and_input() {
    let src = "\
let a: byte[3] = \"abc\";
let i = 1;
read a[i];
put a[0];
put a[i];
put a[2];
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, b"Z"), b"aZc");
}

#[test]
fn pc_backend_runs_functions() {
    let src = "\
fn add(a: byte, b: byte) -> byte {
  let c = a + b;
  return c;
}

let x = add(40, 2);
println x;
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42\n");
}

#[test]
fn pc_backend_runs_void_functions_and_call_statements() {
    let src = "\
fn show(value: byte) {
  print value;
  return;
}

fn show_answer() {
  show(42);
}

show_answer();
puts \" ok\\n\";
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42 ok\n");
}

#[test]
fn void_functions_fall_through_and_byte_results_can_be_discarded() {
    let src = "\
fn trace(value: byte) -> byte {
  put value;
  return value;
}

fn run() {
  trace('A');
}

run();
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn call_statement_arguments_support_nested_byte_calls() {
    let src = "\
fn inc(value: byte) -> byte {
  return value + 1;
}

fn show(value: byte) {
  println value;
}

show(inc(41));
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42\n");
}

#[test]
fn pc_backend_runs_nested_function_calls() {
    let src = "\
fn inc(x: byte) -> byte {
  return x + 1;
}

fn twice(x: byte) -> byte {
  let y = inc(x);
  return inc(y);
}

println twice(40);
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42\n");
}

#[test]
fn function_calls_can_participate_in_expressions() {
    let src = "\
fn add(a: byte, b: byte) -> byte {
  return a + b;
}

fn inc(x: byte) -> byte {
  return x + 1;
}

let x = add(10, 20) + inc(11);
println x;
println add(inc(1), add(2, 3));
if add(1, 1) == 2 {
  puts \"ok\\n\";
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42\n7\nok\n");
}

#[test]
fn function_call_arguments_are_evaluated_left_to_right() {
    let src = "\
fn f(x: byte) -> byte {
  puts \"f\";
  return x;
}

fn g(x: byte) -> byte {
  puts \"g\";
  return x;
}

println f(1) + g(2);
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"fg3\n");
}

#[test]
fn formatted_print_streams_parts_and_arguments_left_to_right() {
    let src = "\
fn trace(value: byte) -> byte {
  put 'X';
  return value;
}

print \"A{:c}B{:d}C{}\\n\", trace('Q'), trace(2), trace(3);
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"AXQBX2CX3\n");
}

#[test]
fn function_frames_do_not_reuse_caller_scope_cells() {
    let src = "\
fn next(n: byte) -> byte {
  return n + 1;
}

for let i = 0; i < 3; i += 1 {
  println i;
  let n = next(10);
  println n;
}
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"0\n11\n1\n11\n2\n11\n");
}

#[test]
fn collatz_example_runs_with_functions() {
    let src = fs::read_to_string("examples/Collatz.sn").unwrap();
    let bf = compile_pc(&src);
    assert_eq!(
        run_bf(&bf, b"7\n"),
        b"input a number: step 0: 7\n\
step 1: 22\n\
step 2: 11\n\
step 3: 34\n\
step 4: 17\n\
step 5: 52\n\
step 6: 26\n\
step 7: 13\n\
step 8: 40\n\
step 9: 20\n\
step 10: 10\n\
step 11: 5\n\
step 12: 16\n\
step 13: 8\n\
step 14: 4\n\
step 15: 2\n\
step 16: 1\n"
    );
}

#[test]
fn reference_examples_keep_ir_size_within_baseline_bounds() {
    let toy_aes = fs::read_to_string("examples/toy_aes_round.sn").unwrap();
    let toy_aes_stats = ir::lower(&resolve(&toy_aes)).stats();
    assert_stats_within(
        toy_aes_stats,
        ir::IrStats {
            blocks: 50,
            reachable_blocks: 50,
            ops: 120,
            empty_blocks: 20,
            trivial_jumps: 4,
            calls: 0,
            branches: 20,
        },
    );

    let collatz = fs::read_to_string("examples/Collatz.sn").unwrap();
    let collatz_stats = ir::lower(&resolve(&collatz)).stats();
    assert_stats_within(
        collatz_stats,
        ir::IrStats {
            blocks: 30,
            reachable_blocks: 24,
            ops: 20,
            empty_blocks: 22,
            trivial_jumps: 14,
            calls: 8,
            branches: 12,
        },
    );
}
