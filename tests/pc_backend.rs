mod common;

use std::fs;

use sane::{bf, lexer, parser, sema};

use common::run_bf;

fn compile_pc(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let program = sema::resolve(&program).unwrap();
    bf::compile_pc(&program).unwrap()
}

#[test]
fn pc_backend_runs_linear_programs() {
    let bf = compile_pc("let x = 'A'; put x; puts \"\\n\";");
    assert_eq!(run_bf(&bf, &[]), b"A\n");
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

print \"A{}B{}\\n\", trace(1), trace(2);
";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"AX1BX2\n");
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
