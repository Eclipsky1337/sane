mod common;

use common::run_bf;
use sane::ast::{Expr, Stmt, ValueType};
use sane::{bf, compile_source, compile_source_with_path, lexer, parser, sema};

fn compile_pc(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let resolved = sema::resolve(&program).unwrap();
    bf::compile_pc(&resolved).unwrap()
}

#[test]
fn fixed_types_and_literals_parse_exact_q16_16_values() {
    let tokens =
        lexer::lex("let one: fixed = 1.0; let half = 0.5; let tenth = 0.1; let min = -32768.0;")
            .unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();

    assert_eq!(ValueType::Fixed.cell_width(), 4);
    assert!(matches!(
        &program.stmts[0],
        Stmt::Let {
            value_type: Some(ValueType::Fixed),
            init: Some(Expr::Fixed(65_536, _)),
            ..
        }
    ));
    assert!(matches!(
        &program.stmts[1],
        Stmt::Let {
            init: Some(Expr::Fixed(32_768, _)),
            ..
        }
    ));
    assert!(matches!(
        &program.stmts[2],
        Stmt::Let {
            init: Some(Expr::Fixed(6_553, _)),
            ..
        }
    ));
}

#[test]
fn fixed_literal_limits_and_unsupported_operators_are_reported() {
    assert!(compile_source("let value = 32768.0;").is_err());
    assert!(compile_source("let value = -32768.0001;").is_err());
    assert!(lexer::lex("let value = 1.;").is_err());
    assert!(lexer::lex("let value = .5;").is_err());

    let error = compile_source_with_path("let value = 1.0 % 0.5;", "fixed.sn").unwrap_err();
    assert!(error.contains("fixed values do not support"));
    let error = compile_source_with_path("let value = 1.0 & 1.0;", "fixed.sn").unwrap_err();
    assert!(error.contains("fixed values do not support"));
    let error = compile_source_with_path("let value = 1.0 + 256;", "fixed.sn").unwrap_err();
    assert!(error.contains("int and fixed values require an explicit cast"));
    let error = compile_source_with_path("let value: fixed; read value;", "fixed.sn").unwrap_err();
    assert!(error.contains("read requires a byte target"));
}

#[test]
fn fixed_conversions_truncate_scale_and_wrap_as_specified() {
    let src = r#"
        let widened: fixed = 3;
        let negative: fixed = -1.75;
        let wrapped: fixed = 65537 as fixed;
        print widened;
        puts " ";
        print negative as int;
        puts " ";
        print negative as byte;
        puts " ";
        print wrapped;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, &[]), b"3.0 -1 255 1.0");

    assert!(compile_source("let value: fixed = 1 as int;").is_err());
    assert!(compile_source("let value: int = 1.0;").is_err());
}

#[test]
fn fixed_arithmetic_comparisons_conditions_and_output_are_exact() {
    let src = r#"
        print 1.5 + -0.25;
        puts " ";
        print 1.5 * -2.25;
        puts " ";
        print -7.0 / 2.0;
        puts " ";
        print 1.0 / 0.0;
        puts " ";
        print 0.1;
        puts " ";
        print 32767.9999847412109375;
        puts " ";
        print -32768.0;
        puts " ";
        print 1.0 + 1;
        puts " ";
        if -1.0 < 0 && 2.0 >= 2 && !0.0 { put 'A'; }
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(
        run_bf(&code, &[]),
        b"1.25 -3.375 -3.5 0.0 0.0999908447265625 32767.9999847412109375 -32768.0 2.0 A"
    );
}

#[test]
fn fixed_truthiness_checks_each_cell_without_wrapping() {
    let src = "let value: fixed = 0.0077972412109375; if value { put 'A'; }";
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, &[]), b"A");
}

#[test]
fn fixed_constants_arrays_and_dynamic_indexes_use_four_cell_values() {
    let src = r#"
        const SCALE = 0.75 * 2.0;
        let values: fixed[3] = [1.25, 2, -0.5];
        let index: byte = 1;
        values[index] *= SCALE;
        print values[0];
        puts " ";
        print values[index];
        puts " ";
        print values[2];
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, &[]), b"1.25 3.0 -0.5");
}

#[test]
fn fixed_dynamic_arrays_use_the_stride_four_primitive() {
    let src = r#"
        let values: fixed[2];
        values[0] = 1.25;
        values[1] = -0.5;
        let index: byte = 1;
        values[index] *= 1.5;
        print values[index];
        puts " ";
        print values[0];
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(run_bf(&code, &[]), b"-0.75 1.25");
    let pc_code = compile_pc(src);
    assert_eq!(run_bf(&pc_code, &[]), b"-0.75 1.25");
}

#[test]
fn fixed_multiplication_and_division_cover_cross_cell_boundaries() {
    let src = r#"
        print 30000.0 * 2.0;
        puts " ";
        print 0.0000152587890625 * 32767.0;
        puts " ";
        print 1.0 / 3.0;
        puts " ";
        print -32768.0 / -1.0;
        puts " ";
        let value: fixed = 5.5;
        value /= -2.0;
        print value;
    "#;
    let code = compile_source(src).unwrap();
    assert_eq!(
        run_bf(&code, &[]),
        b"-5536.0 0.4999847412109375 0.3333282470703125 -32768.0 -2.75"
    );
}

#[test]
fn pc_backend_carries_fixed_arguments_returns_and_spills() {
    let src = r#"
        fn midpoint(left: fixed, right: fixed) -> fixed {
            return (left + right) / 2.0;
        }

        fn twice(value: fixed) -> fixed {
            return value * 2.0;
        }

        let value: fixed = twice(midpoint(1, 5.0));
        println value;
    "#;
    let code = compile_pc(src);
    assert_eq!(run_bf(&code, &[]), b"6.0\n");
}
