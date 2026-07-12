mod common;

use brainwash::{compile_source, compile_source_with_path};
use brainwash::{lexer, parser, sema};
use common::run_bf;

#[test]
fn scratch_starts_after_globals() {
    let tokens = lexer::lex("let x: byte;").unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let symbols = sema::analyze(&program).unwrap();

    assert_eq!(symbols.cell("x"), Some(sema::TEMP_COUNT));
    assert_eq!(sema::TEMP_COUNT, 8);
    assert_eq!(symbols.scratch_base(), 9);
    assert_eq!(symbols.control_base(), 21);
}

#[test]
fn scoped_cells_are_reused_after_scope_exit() {
    let tokens = lexer::lex(
        "
        let a: byte;
        {
            let b: byte;
            let c: byte;
        }
        {
            let d: byte;
        }
        ",
    )
    .unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let symbols = sema::resolve(&program).unwrap().symbols;

    assert_eq!(symbols.cell("a"), Some(sema::TEMP_COUNT));
    assert_eq!(symbols.scratch_base(), sema::TEMP_COUNT + 3);
}

#[test]
fn diagnostics_include_source_location_and_caret() {
    let err = compile_source_with_path("let x = 1\nput x;", "bad.bfc").unwrap_err();
    assert!(err.contains("expected Semi"));
    assert!(err.contains("--> bad.bfc:2:1"), "{err}");
    assert!(err.contains("^"), "{err}");

    let err = compile_source_with_path("put missing;", "bad.bfc").unwrap_err();
    assert!(err.contains("use of undeclared variable `missing`"));
    assert!(err.contains("--> bad.bfc:1:5"), "{err}");
}

#[test]
fn syntax_surface_smoke_test() {
    let src = "
        let x: byte;
        let sum = 0;
        let flag = true;
        let a: byte[3] = [1, 2, 3,];
        let msg: byte[3] = \"ok\\n\";

        read x;
        read a[0];

        {
            let scoped = 1;
            sum += scoped;
            sum -= 1;
        }

        if ~0 == 255 {
            flag = true;
        }

        let i = 0;
        while i < 3 {
            if i == 1 {
                i += 1;
                continue;
            }
            a[i] = a[i] + i;
            sum += a[i];
            i += 1;
        }

        for let j = 0; j < 3; j += 1 {
            sum ^= j;
        }

        loop {
            break;
        }

        sum += x;
        sum -= 1;
        sum *= 2;
        sum /= 2;
        sum %= 255;
        sum &= 0x7f;
        sum |= 0b10000000;
        sum ^= 0x80;
        sum <<= 1;
        sum >>= 1;

        if flag && (sum >= 0) {
            print sum;
            put '\\n';
            put msg[0];
            put msg[1];
            put msg[2];
        } else if false || !flag {
            puts \"bad\\n\";
        } else {
            println 0;
        }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[5, 4]), b"14\nok\n");
}

#[test]
fn constant_generation_outputs_representative_byte_values() {
    for value in [0u8, 1, 2, 7, 31, 63, 64, 127, 128, 191, 200, 255] {
        let src = format!("put {value};");
        let bf = compile_source(&src).unwrap();
        assert_eq!(run_bf(&bf, &[]), &[value], "value={value}");
    }
}

#[test]
fn large_constant_generation_is_compact() {
    let bf = compile_source("put 200;").unwrap();
    assert!(
        bf.len() < 100,
        "expected compact constant generation, got {} bytes: {bf}",
        bf.len()
    );
}

#[test]
fn large_constant_variable_initializer() {
    let bf = compile_source("let x: byte = 200; put x;").unwrap();
    assert_eq!(run_bf(&bf, &[]), &[200]);
}

#[test]
fn assignment_and_addition() {
    let src = "let x: byte = 60; let y: byte; y = x + 5; put y;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn self_assignment_preserves_old_value() {
    let src = "let x: byte = 5; x = x + 1; put x;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[6]);
}

#[test]
fn compound_assignment_ops() {
    let src = "
        let x: byte = 10;
        x += 5;
        put x;
        x -= 3;
        put x;
        x *= 4;
        put x;
        x /= 5;
        put x;
        x %= 7;
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[15, 12, 48, 9, 2]);
}

#[test]
fn bitwise_compound_assignment_ops() {
    let src = "
        let x = 0b1010_1010;
    ";
    assert!(compile_source(src).is_err());

    let src = "
        let x = 0b10101010;
        x &= 204;
        put x;
        x |= 0x03;
        put x;
        x ^= 15;
        put x;
        x <<= 1;
        put x;
        x >>= 2;
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[136, 139, 132, 8, 2]);
}

#[test]
fn binary_and_hex_literals() {
    let src = "put 0b01000001; put 0x42; put 0Xff;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[65, 66, 255]);

    assert!(compile_source("put 0b2;").is_err());
    assert!(compile_source("put 0x100;").is_err());
}

#[test]
fn inferred_let_and_bool_literals() {
    let src = "
        let x = true;
        let y = false;
        put x;
        put y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[1, 0]);

    assert!(compile_source("let x;").is_err());
}

#[test]
fn array_constant_index_get_set() {
    let src = "
        let a: byte[4];
        a[0] = 'A';
        a[1] = 'B';
        a[2] = a[0] + 2;
        put a[0];
        put a[1];
        put a[2];
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABC");
}

#[test]
fn array_dynamic_index_uses_runtime_index() {
    let src = "
        let a: byte[4];
        let i = 0;
        while i < 4 {
            a[i] = 'A' + i;
            i += 1;
        }
        i = 0;
        while i < 4 {
            put a[i];
            i += 1;
        }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCD");
}

#[test]
fn array_read_with_dynamic_index() {
    let src = "
        let a: byte[3];
        let i = 0;
        while i < 3 {
            read a[i];
            i += 1;
        }
        put a[2];
        put a[1];
        put a[0];
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, b"XYZ"), b"ZYX");
}

#[test]
fn array_dynamic_compound_assignment_preserves_rhs_reads() {
    let src = "
        let a: byte[4] = [1, 2, 3, 4];
        let key: byte[4] = [10, 20, 30, 40];
        let i = 0;
        while i < 4 {
            a[i] ^= key[i];
            i += 1;
        }
        put a[0];
        put a[1];
        put a[2];
        put a[3];
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[1 ^ 10, 2 ^ 20, 3 ^ 30, 4 ^ 40]);
}

#[test]
fn array_initializer_sets_elements() {
    let src = "
        let sbox: byte[4] = [0x63, 0x7c, 'A' + 2, 0b01000100,];
        let i = 0;
        while i < 4 {
            put sbox[i];
            i += 1;
        }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[0x63, 0x7c, b'C', b'D']);
}

#[test]
fn array_string_initializer_sets_elements() {
    let src = "
        let msg: byte[6] = \"hi!\\n\\0A\";
        let i = 0;
        while i < 6 {
            put msg[i];
            i += 1;
        }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"hi!\n\0A");
}

#[test]
fn array_semantic_errors() {
    assert!(compile_source("let a: byte[0];").is_err());
    assert!(compile_source("let a: byte[2]; put a[2];").is_err());
    assert!(compile_source("let a: byte[2]; put a;").is_err());
    assert!(compile_source("let x: byte; x[0] = 1;").is_err());
    assert!(compile_source("let a: byte[2] = [1];").is_err());
    assert!(compile_source("let a: byte[3] = \"abcd\";").is_err());
    assert!(compile_source("let a: byte[4] = \"abc\";").is_err());
    assert!(compile_source("let x = 1; let a: byte[1] = [x];").is_err());
}

#[test]
fn does_not_emit_adjacent_clears() {
    let bf = compile_source("let x: byte = 60; let y: byte; y = x + 5; put y;").unwrap();
    assert!(!bf.contains("[-][-]"), "{bf}");
}
