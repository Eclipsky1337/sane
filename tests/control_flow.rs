mod common;

use common::run_bf;
use sane::compile_source;

#[test]
fn if_statement_runs_conditionally() {
    let src = "
        let x: byte = 0;
        if 1 {
            x = 65;
        }
        if 0 {
            x = 66;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}

#[test]
fn if_else_statement_selects_branch() {
    let src = "
        let x: byte = 0;
        let y: byte = 2;
        if y < 2 {
            x = 65;
        } else {
            x = 66;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"B");
}

#[test]
fn while_statement_recomputes_condition() {
    let src = "
        let x: byte = 0;
        while x < 5 {
            x = x + 1;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[5]);
}

#[test]
fn nested_control_flow() {
    let src = "
        let x: byte = 0;
        let y: byte = 0;
        while x < 3 {
            if x != 1 {
                y = y + 2;
            } else {
                y = y + 1;
            }
            x = x + 1;
        }
        put y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[5]);
}

#[test]
fn break_exits_nearest_loop() {
    let src = "
        let x: byte = 0;
        while x < 10 {
            if x == 4 {
                break;
            }
            x = x + 1;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[4]);
}

#[test]
fn continue_skips_rest_of_iteration() {
    let src = "
        let x: byte = 0;
        let y: byte = 0;
        while x < 5 {
            x = x + 1;
            if x == 3 {
                continue;
            }
            y = y + x;
        }
        put y;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[12]);
}

#[test]
fn break_and_continue_are_loop_scoped() {
    let src = "
        let outer: byte = 0;
        let total: byte = 0;
        while outer < 3 {
            let inner: byte = 0;
            while inner < 5 {
                inner = inner + 1;
                if inner == 2 {
                    continue;
                }
                if inner == 4 {
                    break;
                }
                total = total + 1;
            }
            outer = outer + 1;
        }
        put total;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[6]);
}

#[test]
fn break_and_continue_outside_loop_are_errors() {
    assert!(compile_source("break;").is_err());
    assert!(compile_source("continue;").is_err());
}

#[test]
fn block_scope_shadows_and_hides_locals() {
    let src = "
        let x: byte = 65;
        {
            let x: byte = 66;
            put x;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"BA");

    assert!(compile_source("{ let y: byte = 1; } put y;").is_err());
    assert!(compile_source("let x: byte = 1; let x: byte = 2;").is_err());
}

#[test]
fn branch_and_loop_bodies_have_block_scope() {
    let src = "
        let x: byte = 65;
        if 1 {
            let x: byte = 66;
            put x;
        }
        while x == 65 {
            let x: byte = 67;
            put x;
            break;
        }
        put x;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"BCA");

    assert!(compile_source("if 1 { let x: byte = 1; } put x;").is_err());
    assert!(compile_source("while 0 { let x: byte = 1; } put x;").is_err());
}

#[test]
fn for_loop_counts_and_scopes_initializer() {
    let src = "
        let sum: byte = 0;
        for let i: byte = 0; i < 5; i += 1 {
            sum += i;
        }
        put sum;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[10]);

    assert!(compile_source("for let i: byte = 0; i < 1; i += 1 {} put i;").is_err());
}

#[test]
fn for_continue_runs_step_and_break_exits() {
    let src = "
        let sum: byte = 0;
        for let i: byte = 0; i < 8; i += 1 {
            if i == 2 {
                continue;
            }
            if i == 5 {
                break;
            }
            sum += i;
        }
        put sum;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), &[8]);
}

#[test]
fn else_if_and_loop_work_with_break_continue() {
    let src = "
        let x = 0;
        let out = 0;
        loop {
            x += 1;
            if x == 1 {
                continue;
            } else if x == 2 {
                out = 65;
            } else {
                break;
            }
        }
        put out;
    ";
    let bf = compile_source(src).unwrap();
    assert_eq!(run_bf(&bf, &[]), b"A");
}
