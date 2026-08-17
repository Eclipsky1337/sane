mod common;

use common::run_bf;
use sane::{bf, compile_source, compile_source_with_path, lexer, parser, sema};

fn compile_pc(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let resolved = sema::resolve(&program).unwrap();
    bf::compile_pc(&resolved).unwrap()
}

#[test]
fn input_parses_byte_int_and_raw_byte_fields() {
    let src = r#"
        let left: int = 99;
        let right: byte = 9;
        let mark: byte = '?';
        let matched = input("{} {} {:c}", left, right, mark);
        print "{} {} {} {:c}\n", matched, left, right, mark;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"-123 45 Z\n"), b"3 -123 45 Z\n");
}

#[test]
fn input_accepts_signed_int_boundaries() {
    let src = r#"
        let minimum: int;
        let maximum: int;
        let matched = input("{},{}", minimum, maximum);
        print "{} {} {}\n", matched, minimum, maximum;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(
        run_bf(&code, b"-2147483648,2147483647\n"),
        b"2 -2147483648 2147483647\n"
    );
}

#[test]
fn input_failure_preserves_destination_and_conversion_count() {
    let src = r#"
        let first: int = 111;
        let second: int = 222;
        let matched = input("{}:{}", first, second);
        print "{} {} {}\n", matched, first, second;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"12x34\n"), b"1 12 222\n");
    assert_eq!(run_bf(&code, b"12:x\n"), b"1 12 222\n");
    assert_eq!(run_bf(&code, b"x:34\n"), b"0 111 222\n");
    assert_eq!(run_bf(&code, b""), b"0 111 222\n");
}

#[test]
fn input_rejects_decimal_overflow_without_writing() {
    let src = r#"
        let small: byte = 7;
        let wide: int = 8;
        let matched = input("{} {}", small, wide);
        print "{} {} {}\n", matched, small, wide;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"256 1\n"), b"0 7 8\n");
    assert_eq!(run_bf(&code, b"25 2147483648\n"), b"1 25 8\n");
    assert_eq!(run_bf(&code, b"25 -2147483649\n"), b"1 25 8\n");
}

#[test]
fn trailing_decimal_consumes_line_ending() {
    let src = r#"
        let value: int;
        let next: byte;
        let matched = input("{}", value);
        read next;
        print "{} {} {:c}\n", matched, value, next;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"42\nZ"), b"1 42 Z\n");
    assert_eq!(run_bf(&code, b"42\rZ"), b"1 42 Z\n");
    assert_eq!(run_bf(&code, b"42"), b"1 42 \0\n");
    assert_eq!(run_bf(&code, b"42xZ"), b"0 0 Z\n");
}

#[test]
fn input_matches_whitespace_and_escaped_braces_exactly() {
    let src = r#"
        let value: byte = 9;
        let matched = input("{{{}}}", value);
        print "{} {}\n", matched, value;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"{42}\n"), b"1 42\n");
    assert_eq!(run_bf(&code, b"{42]\n"), b"1 42\n");

    let whitespace = compile_source(
        "let a: byte = 8; let b: byte = 9; let n = input(\"{} {}\", a, b); print \"{} {} {}\\n\", n, a, b;",
    )
    .unwrap();
    assert_eq!(run_bf(&whitespace, b"1\t2\n"), b"1 1 9\n");
}

#[test]
fn raw_input_field_fails_on_eof_without_writing() {
    let src = r#"
        let mark: byte = 'X';
        let matched = input("{:c}", mark);
        print "{} {:c}\n", matched, mark;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b""), b"0 X\n");
}

#[test]
fn input_updates_dynamic_array_targets_left_to_right() {
    let src = r#"
        let index: byte = 0;
        let bytes: byte[2] = [8, 9];
        let ints: int[2] = [10, 11];
        let matched = input("{} {} {}", index, bytes[index], ints[index]);
        print "{} {} {} {}\n", matched, index, bytes[1], ints[1];
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"1 65 -123\n"), b"3 1 65 -123\n");
    let pc_code = compile_pc(src);
    assert_eq!(run_bf(&pc_code, b"1 65 -123\n"), b"3 1 65 -123\n");
}

#[test]
fn input_writes_stride_four_int_array_targets() {
    let src = r#"
        let values: int[2];
        let index: byte = 1;
        let matched = input("{}", values[index]);
        print "{} {}\n", matched, values[index];
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, b"-123456\n"), b"1 -123456\n");
    let pc_code = compile_pc(src);
    assert_eq!(run_bf(&pc_code, b"-123456\n"), b"1 -123456\n");
}

#[test]
fn input_behaves_the_same_in_pc_backend() {
    let src = r#"
        fn identity(value: int) -> int { return value; }
        let first: int = 1;
        let second: byte = 2;
        let matched = input("[{}|{}]", first, second);
        print "{} {} {}\n", matched, identity(first), second;
    "#;
    let code = compile_pc(src);
    assert_eq!(run_bf(&code, b"[-77|240]\n"), b"2 -77 240\n");
    assert_eq!(run_bf(&code, b"[-77/240]\n"), b"1 -77 2\n");
}

#[test]
fn input_frontend_rejects_invalid_formats_and_destinations() {
    assert!(compile_source("let x: byte; let n = input(\"{}\");").is_err());
    assert!(compile_source("let x: byte; let n = input(\"\", x);").is_err());
    assert!(compile_source("let x: int; let n = input(\"{:c}\", x);").is_err());
    assert!(compile_source("let x: fixed; let n = input(\"{}\", x);").is_err());
    assert!(compile_source("let fmt: byte; let x: byte; let n = input(fmt, x);").is_err());

    let error =
        compile_source_with_path("const X = 0; let n = input(\"{}\", X);", "input.sn").unwrap_err();
    assert!(error.contains("constant"));
}
