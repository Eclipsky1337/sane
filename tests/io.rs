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
