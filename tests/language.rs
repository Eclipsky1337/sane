mod common;

use common::run_bf;
use sane::ast::{Expr, Stmt, UnOp, ValueType};
use sane::{compile_source, compile_source_with_path};
use sane::{lexer, parser, sema};

#[test]
fn scalar_types_define_byte_int_and_fixed_storage_widths() {
    let byte = sema::ValueLayout::byte(8);
    let int = sema::ValueLayout {
        base: 12,
        value_type: ValueType::Int,
    };

    assert_eq!(ValueType::Byte.cell_width(), 1);
    assert_eq!(ValueType::Int.cell_width(), 4);
    assert_eq!(ValueType::Fixed.cell_width(), 4);
    assert_eq!(byte.cells(), 8..9);
    assert_eq!(int.cells(), 12..16);

    let byte_array = sema::ArrayLayout {
        base: 20,
        len: 1,
        value_type: ValueType::Byte,
    };
    let wide_array = sema::ArrayLayout {
        base: 20,
        len: 1,
        value_type: ValueType::Int,
    };
    assert!(!byte_array.uses_wide_array_primitive());
    assert_eq!(byte_array.element(0).base, 24);
    assert!(wide_array.uses_wide_array_primitive());
    assert_eq!(wide_array.element(0).base, 20 + sema::WIDE_ARRAY_PREFIX);
}

#[test]
fn int_declarations_literals_negation_and_casts_parse() {
    let tokens = lexer::lex(
        "let bits: int = 0xffffffff; let min = -2147483648; let low: byte = bits as byte;",
    )
    .unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();

    assert!(matches!(
        &program.stmts[0],
        Stmt::Let {
            value_type: Some(ValueType::Int),
            init: Some(Expr::Int(0xffff_ffff, _, _)),
            ..
        }
    ));
    assert!(matches!(
        &program.stmts[1],
        Stmt::Let {
            value_type: None,
            init: Some(Expr::Unary {
                op: UnOp::Neg,
                expr,
                ..
            }),
            ..
        } if matches!(expr.as_ref(), Expr::Int(2_147_483_648, _, _))
    ));
    assert!(matches!(
        &program.stmts[2],
        Stmt::Let {
            value_type: Some(ValueType::Byte),
            init: Some(Expr::Cast {
                target: ValueType::Byte,
                ..
            }),
            ..
        }
    ));
}

#[test]
fn int_frontend_reports_staged_lowering_and_literal_limits() {
    assert!(lexer::lex("let value = 4294967296;").is_err());
    assert!(compile_source("let value = 2147483648;").is_err());
    assert!(compile_source("let value = -2147483649;").is_err());

    let bf = compile_source(
        "let minimum: int = -2147483648; let bits: int = 0xffffffff; if minimum == 0x80000000 && bits == -1 { put 'A'; }",
    )
    .unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn int_scalars_initialize_assign_copy_and_cast() {
    let src = "
        let first: int = 0x12345641;
        let copy: int = first;
        let widened: int = 'Z';
        let inferred = 0xabcdef42;
        first = inferred;
        let low: byte = copy as byte;
        put low;
        put widened as byte;
        put first as byte;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"AZB");
}

#[test]
fn int_to_byte_requires_an_explicit_cast() {
    let error = compile_source_with_path("let wide: int = 300; let narrow: byte = wide;", "int.sn")
        .unwrap_err();
    assert!(error.contains("cannot assign Int expression to Byte variable"));
}

#[test]
fn int_add_sub_equality_and_conditions_use_all_four_cells() {
    let src = "
        let value: int = 255;
        value = value + 1;
        if value == 256 { put 'A'; }

        value = value + 0x0000ff00;
        if value == 65536 { put 'B'; }

        value = value - 1;
        if value == 65535 { put 'C'; }

        let wrapped: int = 0xffffffff + 1;
        if wrapped == 0 { put 'D'; }

        let borrowed: int = 0;
        borrowed = borrowed - 1;
        if borrowed == 0xffffffff { put 'E'; }
        if borrowed != 0 { put 'F'; }
        if borrowed { put 'G'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCDEFG");
}

#[test]
fn int_mixed_arithmetic_promotes_bytes_and_supports_negation() {
    let src = "
        let wide = 300;
        let mixed: int = 1 + wide;
        if mixed == 301 { put 'A'; }
        let small: byte = 2;
        mixed = small + wide;
        if mixed == 302 { put 'B'; }
        let negative: int = -small;
        if negative == 0xfffffffe { put 'C'; }
        negative = -wide;
        if negative == 0xfffffed4 { put 'D'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCD");
}

#[test]
fn int_bitwise_logical_and_signed_ordering_work() {
    let src = "
        let left: int = 0x12345678;
        let right: int = 0x00ff00ff;
        if (left & right) == 0x00340078 { put 'A'; }
        if (left | right) == 0x12ff56ff { put 'B'; }
        if (left ^ right) == 0x12cb5687 { put 'C'; }
        if (~left) == 0xedcba987 { put 'D'; }

        let negative: int = -1;
        let zero: int = 0;
        let minimum: int = 0x80000000;
        let maximum: int = 0x7fffffff;
        if negative < zero { put 'E'; }
        if zero > negative { put 'F'; }
        if minimum < negative { put 'G'; }
        if maximum >= zero { put 'H'; }
        if negative <= negative { put 'I'; }
        if negative && maximum { put 'J'; }
        if zero || maximum { put 'K'; }
        if !zero { put 'L'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCDEFGHIJKL");
}

#[test]
fn int_shifts_cross_cells_and_preserve_sign() {
    let src = "
        let value: int = 1;
        value = value << 8;
        if value == 256 { put 'A'; }
        value = value << 16;
        if value == 0x01000000 { put 'B'; }
        value = value >> 20;
        if value == 16 { put 'C'; }

        let negative: int = 0x80000000;
        negative = negative >> 31;
        if negative == 0xffffffff { put 'D'; }

        let zeroed: int = 0x12345678 << 32;
        if zeroed == 0 { put 'E'; }
        let signed: int = -2;
        signed = signed >> 1;
        if signed == -1 { put 'F'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCDEF");
}

#[test]
fn int_multiplication_wraps_across_four_cells() {
    let src = "
        let product: int = 300 * 400;
        if product == 120000 { put 'A'; }
        let negative: int = -3 * 7;
        if negative == -21 { put 'B'; }
        let wrapped: int = 0x00010000 * 0x00010000;
        if wrapped == 0 { put 'C'; }
        let mixed: int = 5 * 300;
        if mixed == 1500 { put 'D'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCD");
}

#[test]
fn int_multiplication_does_not_treat_byte_sum_256_as_zero() {
    let src = "let left: int = 1; let right: int = 32767; print left * right;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"32767");
}

#[test]
fn int_division_and_remainder_are_signed_and_deterministic() {
    let src = "
        let a: int = 7;
        let b: int = 3;
        if a / b == 2 && a % b == 1 { put 'A'; }
        a = -7;
        if a / b == -2 && a % b == -1 { put 'B'; }
        b = -3;
        if a / b == 2 && a % b == -1 { put 'C'; }
        a = 7;
        if a / b == -2 && a % b == 1 { put 'D'; }

        let large: int = 0x7fffffff;
        let scale: int = 65536;
        if large / scale == 32767 && large % scale == 65535 { put 'E'; }

        let zero: int = 0;
        if large / zero == 0 && large % zero == large { put 'F'; }
        let minimum: int = 0x80000000;
        if minimum / -1 == minimum && minimum % -1 == 0 { put 'G'; }
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABCDEFG");
}

#[test]
fn int_function_types_reject_implicit_narrowing() {
    let err =
        compile_source("fn take(value: byte) { } let wide: int = 300; take(wide);").unwrap_err();
    assert!(err.contains("cannot pass Int expression to Byte parameter"));

    let err = compile_source("fn narrow() -> byte { return 300; }").unwrap_err();
    assert!(err.contains("cannot return int expression from byte function"));
}

#[test]
fn int_arrays_use_four_cell_elements_and_dynamic_indexes() {
    let src = r#"
        let values: int[4] = [1, 65536, -2, 0x7fffffff];
        if values[0] == 1 && values[1] == 65536 { put 'A'; }
        values[2] = values[1] + 5;
        let index: byte = 2;
        if values[index] == 65541 { put 'B'; }
        values[index + 1] = -2147483647;
        if values[3] == -2147483647 { put 'C'; }
    "#;
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ABC");
}

#[test]
fn dynamic_wide_array_access_preserves_aliases_and_repeated_indexes() {
    let src = r#"
        let values: int[4];
        values[0] = 100;
        values[1] = -2;
        values[2] = 65536;
        values[3] = 7;
        let source: byte = 1;
        let destination: byte = 0;
        values[destination] = values[source];
        values[source + 1] = values[destination] + 5;
        print "{} {} {} {}\n", values[destination], values[source], values[2], values[3];
    "#;
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"-2 -2 3 7\n");
}

#[test]
fn dynamic_wide_array_primitive_keeps_growth_below_linear_selection() {
    let compile = |len: usize| {
        compile_source(&format!(
            "let values: int[{len}];\
             let index: byte = {};\
             values[index] = 123456;\
             let value: int = values[index];\
             put value as byte;",
            len - 1
        ))
        .unwrap()
    };

    let fourteen = compile(14);
    let thirty_two = compile(32);
    assert!(thirty_two.len() < fourteen.len() * 2);
}

#[test]
fn inferred_int_arrays_and_element_types_are_checked() {
    let src = "let values = [1, 300, -2]; if values[1] == 300 { put 'A'; }";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");

    let err = compile_source("let values: byte[1] = [300];").unwrap_err();
    assert!(err.contains("cannot initialize Byte array with Int expression"));
}

#[test]
fn scratch_starts_after_globals() {
    let tokens = lexer::lex("let x: byte;").unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let symbols = sema::analyze(&program).unwrap();

    assert_eq!(symbols.cell("x"), Some(sema::TEMP_COUNT));
    assert_eq!(
        symbols.layout("x"),
        Some(sema::ValueLayout::byte(sema::TEMP_COUNT))
    );
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
fn library_compiles_functions_with_pc_backend() {
    let bf = compile_source("fn f() -> byte { return 42; } println f();").unwrap();
    assert_eq!(run_bf(&bf, &[]), b"42\n");
}

#[test]
fn recursive_functions_are_rejected() {
    let src = "\
fn f() -> byte { return g(); }
fn g() -> byte { return f(); }
put f();
";
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let err = sema::resolve(&program).unwrap_err();
    assert!(
        err.message
            .contains("recursive function calls are not supported"),
        "{:?}",
        err
    );
}

#[test]
fn direct_recursive_functions_are_rejected() {
    let src = "fn f() -> byte { return f(); } put f();";
    let tokens = lexer::lex(src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let err = sema::resolve(&program).unwrap_err();
    assert!(
        err.message
            .contains("recursive function calls are not supported"),
        "{:?}",
        err
    );
}

#[test]
fn function_declaration_errors_are_reported() {
    let cases = [
        (
            "fn f() -> byte { return 1; } fn f() -> byte { return 2; }",
            "function `f` already declared",
        ),
        (
            "fn f(a: byte, a: byte) -> byte { return a; }",
            "parameter `a` already declared",
        ),
        ("return 1;", "`return` outside function"),
        (
            "fn f() -> byte { return 1; } put missing();",
            "call to undeclared function `missing`",
        ),
        (
            "fn f(a: byte) -> byte { return a; } put f(1, 2);",
            "function `f` expects 1 arguments, got 2",
        ),
        (
            "fn f() { return 1; } f();",
            "void function cannot return a value",
        ),
        (
            "fn f() -> byte { return; } f();",
            "byte function must return a value",
        ),
        (
            "fn f() -> byte { if true { return 1; } } put f();",
            "byte function `f` may fall through without returning a value",
        ),
        (
            "fn f() {} let value = f();",
            "void function `f` cannot be used as a value",
        ),
        ("return;", "`return` outside function"),
    ];

    for (src, expected) in cases {
        let tokens = lexer::lex(src).unwrap();
        let mut parser = parser::Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let err = sema::resolve(&program).unwrap_err();
        assert!(err.message.contains(expected), "{:?}", err);
    }
}

#[test]
fn function_call_depth_is_limited() {
    let mut src = String::new();
    for i in 0..17 {
        let next = i + 1;
        if i == 16 {
            src.push_str(&format!("fn f{i}() -> byte {{ return 1; }}\n"));
        } else {
            src.push_str(&format!("fn f{i}() -> byte {{ return f{next}(); }}\n"));
        }
    }
    src.push_str("put f0();");

    let tokens = lexer::lex(&src).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    let err = sema::resolve(&program).unwrap_err();
    assert!(
        err.message
            .contains("function call depth 17 exceeds limit 16"),
        "{:?}",
        err
    );
}

#[test]
fn diagnostics_include_source_location_and_caret() {
    let err = compile_source_with_path("let x = 1\nput x;", "bad.sn").unwrap_err();
    assert!(err.contains("expected Semi"));
    assert!(err.contains("--> bad.sn:2:1"), "{err}");
    assert!(err.contains("^"), "{err}");

    let err = compile_source_with_path("put missing;", "bad.sn").unwrap_err();
    assert!(err.contains("use of undeclared variable `missing`"));
    assert!(err.contains("--> bad.sn:1:5"), "{err}");
}

#[test]
fn formatted_print_errors_are_reported() {
    let cases = [
        (
            "print \"{} {}\", 1;",
            "format string has 2 placeholders, but 1 arguments were provided",
        ),
        (
            "print \"{}\", 1, 2;",
            "format string has 1 placeholders, but 2 arguments were provided",
        ),
        ("print \"value: {\", 1;", "unmatched brace in format string"),
        ("print \"{:x}\", 1;", "unsupported format specifier"),
        ("print \"{:dd}\", 1;", "invalid format placeholder"),
        (
            "fn show() {} print \"{}\", show();",
            "void function `show` cannot be used as a value",
        ),
    ];

    for (src, expected) in cases {
        let tokens = lexer::lex(src).unwrap();
        let mut parser = parser::Parser::new(tokens);
        match parser.parse_program() {
            Ok(program) => {
                let error = sema::resolve(&program).unwrap_err();
                assert!(error.message.contains(expected), "{error:?}");
            }
            Err(error) => assert!(error.message.contains(expected), "{error:?}"),
        }
    }
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
    let baseline = compile_source("put 1;").unwrap();
    let bf = compile_source("put 200;").unwrap();
    assert!(
        bf.len() < baseline.len() + 100,
        "expected compact constant generation: baseline={}, generated={}",
        baseline.len(),
        bf.len(),
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
fn inferred_array_declarations_use_initializer_lengths() {
    let src = r#"
        let values = [64, 1 + 1, 'C'];
        let message = "A\n";
        put values[0] + values[1] - 1;
        put values[2];
        put message[0];
        put message[1];
    "#;
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"ACA\n");
}

#[test]
fn inferred_empty_array_requires_an_element_type() {
    let err = compile_source("let values = [];").unwrap_err();
    assert!(
        err.contains("cannot infer the element type of an empty array"),
        "{err}"
    );
}

#[test]
fn inferred_array_initializers_remain_constant() {
    let err = compile_source("let value = 1; let values = [value];").unwrap_err();
    assert!(
        err.contains("array initializer elements must be constant bytes"),
        "{err}"
    );
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
