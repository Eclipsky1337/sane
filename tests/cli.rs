mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::run_bf;

#[test]
fn sanec_help_describes_arguments_and_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_sanec"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());

    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage: sanec [source.sn] [-o out.bf] [-s]"));
    assert!(help.contains("source.sn       Read Sane source from file"));
    assert!(help.contains("-o <file>       Write Brainfuck output to <file>"));
    assert!(help.contains("-s              Add BF-safe symbol table comments"));
}

#[test]
fn sanei_help_describes_arguments_and_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());

    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage: sanei <program.bf>"));
    assert!(help.contains("program.bf      Run Brainfuck program from <program.bf>"));
    assert!(help.contains("Non-Brainfuck characters are ignored"));
    assert!(help.contains("Program input is read from stdin"));
}

#[test]
fn sanec_can_annotate_symbols_in_brainfuck_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sanec"))
        .arg("-s")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"let x = 'A'; let msg: byte[2] = \"hi\"; put x;")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let bf = String::from_utf8(output.stdout).unwrap();
    assert!(bf.starts_with("SANE SYMBOLS\n"));
    assert!(bf.contains("x CELL 8\n"));
    assert!(bf.contains("msg ARRAY BASE 9 LEN 2 DATA CELLS 13 TO 14\n"));
    assert!(bf.contains("END SANE SYMBOLS\n"));

    let annotation = bf.split("END SANE SYMBOLS\n").next().unwrap();
    assert!(
        !annotation
            .bytes()
            .any(|byte| matches!(byte, b'+' | b'-' | b'<' | b'>' | b'[' | b']' | b'.' | b',')),
        "{annotation}"
    );
    assert_eq!(run_bf(&bf, &[]), b"A");
}
