mod common;

use common::run_bf;
use sane::{bf, compile_source, lexer, parser, sema};

fn resolve(src: &str) -> Result<sema::ResolvedProgram, sane::diagnostic::Diagnostic> {
    let tokens = lexer::lex(src)?;
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program()?;
    sema::resolve(&program)
}

fn compile_pc(src: &str) -> String {
    bf::compile_pc(&resolve(src).unwrap()).unwrap()
}

#[test]
fn constants_evaluate_byte_expressions_at_compile_time() {
    let src = "
        const BASE = 40;
        const ANSWER = BASE + 2;
        const WRAPPED = 255 + 2;
        const MASK = ~0;
        println ANSWER;
        println WRAPPED;
        println MASK;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"42\n1\n255\n");
}

#[test]
fn constants_are_block_scoped_and_may_shadow() {
    let src = "
        const VALUE = 'A';
        {
            const VALUE = 'B';
            put VALUE;
        }
        put VALUE;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"BA");
}

#[test]
fn constants_work_in_array_lengths_initializers_and_indexes() {
    let src = "
        const LEN = 3;
        const FIRST = 'A';
        const LAST = LEN - 1;
        let data: byte[LEN] = [FIRST, FIRST + 1, FIRST + 2];
        put data[0];
        put data[LAST];
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"AC");
}

#[test]
fn constants_do_not_allocate_tape_cells_or_debug_symbols() {
    let program = resolve("const VALUE = 42; let result = VALUE;").unwrap();
    assert_eq!(program.symbols.cell("VALUE"), None);
    assert_eq!(program.symbols.cell("result"), Some(sema::TEMP_COUNT));
    assert_eq!(program.symbols.scratch_base(), sema::TEMP_COUNT + 1);
}

#[test]
fn functions_can_use_global_and_local_constants() {
    let src = "
        const BASE = 40;

        fn answer() -> byte {
            const OFFSET = 2;
            return BASE + OFFSET;
        }

        println answer();
    ";
    let bf = compile_pc(src);
    assert_eq!(run_bf(&bf, &[]), b"42\n");
}

#[test]
fn invalid_constant_expressions_are_rejected() {
    let cases = [
        (
            "const FIRST = SECOND; const SECOND = 2;",
            "undeclared or forward name `SECOND`",
        ),
        (
            "fn value() -> byte { return LATER; } const LATER = 1; put value();",
            "undeclared variable `LATER`",
        ),
        ("const VALUE = VALUE;", "undeclared or forward name `VALUE`"),
        (
            "let value = 1; const COPY = value;",
            "constant expression cannot use variable `value`",
        ),
        (
            "let data = [1]; const VALUE = data[0];",
            "constant expression cannot access an array",
        ),
        (
            "fn value() -> byte { return 1; } const VALUE = value();",
            "constant expression cannot call a function",
        ),
        ("const ZERO = 0; let data: byte[ZERO];", "greater than zero"),
        ("const VALUE = 1; VALUE = 2;", "`VALUE` is a constant"),
        (
            "{ const VALUE = 1; } put VALUE;",
            "undeclared variable `VALUE`",
        ),
    ];

    for (src, expected) in cases {
        let error = resolve(src).unwrap_err();
        assert!(error.message.contains(expected), "{error:?}");
    }
}
