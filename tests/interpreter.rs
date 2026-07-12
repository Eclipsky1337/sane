use sane::interpreter::{Op, Program, run_source};

#[test]
fn bf_interpreter_runs_basic_programs() {
    let mut output = Vec::new();
    run_source("+++++[>+++++++++++++<-]>.", &[][..], &mut output).unwrap();
    assert_eq!(output, b"A");
}

#[test]
fn bf_interpreter_reads_and_writes_bytes() {
    let mut output = Vec::new();
    run_source(",.,.", b"AZ".as_slice(), &mut output).unwrap();
    assert_eq!(output, b"AZ");
}

#[test]
fn bf_optimizer_combines_simple_ops() {
    let program = Program::parse("+++-->>><").unwrap();
    assert_eq!(program.ops(), &[Op::Add(1), Op::Move(2)]);
}

#[test]
fn bf_optimizer_recognizes_clear_loop() {
    let program = Program::parse("+++[-].").unwrap();
    assert_eq!(program.ops(), &[Op::Add(3), Op::Clear, Op::Output]);
}

#[test]
fn bf_optimizer_recognizes_add_mul_loop() {
    let program = Program::parse("[->++>+++<<]").unwrap();
    assert_eq!(program.ops(), &[Op::AddMul(vec![(1, 2), (2, 3)])]);
}

#[test]
fn bf_interpreter_reports_unmatched_brackets() {
    assert!(Program::parse("[").is_err());
    assert!(Program::parse("]").is_err());
}
