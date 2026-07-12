mod common;

use brainwash::compile_source;
use common::run_bf;

#[test]
fn equality_and_not() {
    let src = "
        put 1 == 1;
        put 1 != 1;
        put 2 != 1;
        put !0;
        put !7;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[1, 0, 1, 1, 0]);
}

#[test]
fn runtime_equality_and_inequality_samples() {
    let src = "
        let x: byte;
        let y: byte;
        read x;
        read y;
        put x == y;
        put x != y;
    ";
    let bf = compile_source(src).unwrap();

    for x in [0, 1, 2, 7, 31, 127, 128, 255] {
        for y in [0, 1, 2, 7, 31, 127, 128, 255] {
            assert_eq!(run_bf(&bf, &[x, y]), [u8::from(x == y), u8::from(x != y)]);
        }
    }
}

#[test]
fn comparison_precedence() {
    let src = "put 1 + 2 == 3; put 1 + (2 != 3);";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[1, 2]);
}

#[test]
fn runtime_ordered_comparisons() {
    let src = "
        let x: byte = 1;
        let y: byte = 2;
        put x < y;
        put y < x;
        put y <= y;
        put y > x;
        put x >= y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[1, 0, 1, 1, 0]);
}

#[test]
fn logical_ops() {
    let src = "put 0 && 1; put 2 && 3; put 0 || 0; put 0 || 5; put 6 || 7;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[0, 1, 0, 1, 1]);
}

#[test]
fn mul_div_mod() {
    let src = "put 6 * 7; put 14 / 3; put 14 % 3; put 10 / 0; put 10 % 0;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[42, 4, 2, 0, 10]);
}

#[test]
fn runtime_division_and_modulo() {
    let src = "
        let x: byte = 14;
        let y: byte = 3;
        let z: byte = 0;
        put x / y;
        put x % y;
        put x / z;
        put x % z;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[4, 2, 0, 14]);
}

#[test]
fn runtime_variable_division_and_modulo_samples() {
    let src = "
        let x: byte;
        let y: byte;
        read x;
        read y;
        put x / y;
        put x % y;
    ";
    let bf = compile_source(src).unwrap();

    for divisor in [0u8, 1, 2, 3, 5, 7, 16, 31, 64, 127, 255] {
        for value in [0, 1, 2, 3, 7, 14, 31, 63, 127, 128, 191, 255] {
            let expected = if divisor == 0 {
                [0, value]
            } else {
                [value / divisor, value % divisor]
            };
            assert_eq!(
                run_bf(&bf, &[value, divisor]),
                expected,
                "value={value} divisor={divisor}"
            );
        }
    }
}

#[test]
fn runtime_const_arithmetic() {
    let src = "
        let x: byte;
        read x;
        put x + 5;
        put x - 5;
        put 5 - x;
        put x * 7;
        put 7 * x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[10]), &[15, 5, 251, 70, 70]);
}

#[test]
fn bitwise_ops() {
    let src = "
        let x: byte;
        let y: byte;
        read x;
        read y;
        put x & y;
        put x | y;
        put x ^ y;
        put ~x;
        put x << 1;
        put x >> 2;
        put x << y;
        put x >> y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(
        run_bf(&bf, &[0b1010_1100, 3]),
        &[
            0b1010_1100 & 3,
            0b1010_1100 | 3,
            0b1010_1100 ^ 3,
            !0b1010_1100u8,
            0b0101_1000,
            0b0010_1011,
            0b0110_0000,
            0b0001_0101,
        ]
    );
}

#[test]
fn large_shifts_produce_zero() {
    let src = "
        let x: byte = 255;
        let y: byte = 8;
        put x << y;
        put x >> y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[0, 0]);
}

#[test]
fn runtime_const_division_and_modulo() {
    for divisor in [0u8, 1, 2, 3, 5, 7, 16, 31, 64, 127, 255] {
        let src = format!(
            "
            let x: byte;
            read x;
            put x / {divisor};
            put x % {divisor};
            "
        );
        let bf = compile_source(&src).unwrap();

        for value in [0u8, 1, 2, 3, 7, 14, 31, 63, 127, 128, 191, 255] {
            let expected = if divisor == 0 {
                [0, value]
            } else {
                [value / divisor, value % divisor]
            };
            assert_eq!(
                run_bf(&bf, &[value]),
                expected,
                "value={value} divisor={divisor}"
            );
        }
    }
}

#[test]
fn const_division_emits_smaller_bf_than_variable_division() {
    let const_bf = compile_source("let x: byte; read x; put x / 3; put x % 3;").unwrap();
    let variable_bf =
        compile_source("let x: byte; let y: byte = 3; read x; put x / y; put x % y;").unwrap();

    assert!(
        const_bf.len() < variable_bf.len(),
        "const={}, variable={}",
        const_bf.len(),
        variable_bf.len()
    );
}

#[test]
fn arithmetic_precedence() {
    let src = "put 2 + 3 * 4; put (2 + 3) * 4; put 14 % 5 + 48;";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[14, 20, 52]);
}
