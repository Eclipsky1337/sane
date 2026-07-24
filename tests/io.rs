mod common;

use common::run_bf;
use sane::compile_source;

#[test]
fn put_initialized_byte() {
    let bf = compile_source("let x: byte = 65; put x;").unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn character_and_string_literals() {
    let src = "put 'A'; put '\\n'; puts \"B\\\\C\\n\";";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A\nB\\C\n");
}

#[test]
fn print_and_println_decimal_bytes() {
    let src = "
        print 0;
        put ' ';
        print 5;
        put ' ';
        print 42;
        put ' ';
        print 100;
        put ' ';
        print 255;
        println 7;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"0 5 42 100 2557\n");
}

#[test]
fn formatted_print_combines_literals_and_decimal_expressions() {
    let src = r#"
        let round = 3;
        let value = 42;
        print "round: {} value: {}\n", round, value;
        print "result: {{{}}}\n", value + 1;
        print "done\n";
    "#;
    let bf = compile_source(src).unwrap();
    assert_eq!(
        run_bf(&bf, &[]),
        b"round: 3 value: 42\nresult: {43}\ndone\n"
    );
}

#[test]
fn formatted_print_supports_byte_and_explicit_decimal_output() {
    let src = r#"
        print "{:c}:{:d}:{}\n", 'A', 42, 7;
        print "{{{:c}}}\n", 'Z';
    "#;
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A:42:7\n{Z}\n");
}

#[test]
fn put_expression() {
    let src = "let x: byte = 60; put x + 5;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn read_byte() {
    let src = "let x: byte; read x; put x;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, b"Z"), b"Z");
}
