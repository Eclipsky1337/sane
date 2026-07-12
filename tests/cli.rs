mod common;

use std::io::Write;
use std::process::{Command, Stdio};
use std::{env, fs};

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
    assert!(help.contains("Usage: sanei [-d] <program.bf>"));
    assert!(help.contains("program.bf      Run Brainfuck program from <program.bf>"));
    assert!(help.contains("-d              Run program in interactive debug mode"));
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

#[test]
fn sanei_debugger_supports_stepping_breakpoints_and_tape_inspection() {
    let path = env::temp_dir().join(format!("sane-debug-{}.bf", std::process::id()));
    fs::write(&path, "++>+<.").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"s 2\n\
              pc\n\
              b 5\n\
              breakpoints\n\
              c\n\
              x/2d 0\n\
              x/xw 0\n\
              q\n",
        )
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("sanei debug mode"));
    assert!(stdout.contains("pc=2 inst='>' ptr=0 cell[0]=2 state=paused"));
    assert!(stdout.contains("pc=2"));
    assert!(stdout.contains("breakpoint set at 5"));
    assert!(stdout.contains("breakpoint pc=5"));
    assert!(stdout.contains("0 2"));
    assert!(stdout.contains("1 1"));
    assert!(stdout.contains("0 0x00000102"));
}

#[test]
fn sanei_debugger_next_stops_before_matching_instruction() {
    let path = env::temp_dir().join(format!("sane-debug-next-{}.bf", std::process::id()));
    fs::write(&path, "++>+<.").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"next >\npc\ns\nnext .\ninst\nq\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("next pc=2 inst='>'"));
    assert!(stdout.contains("pc=2"));
    assert!(stdout.contains("pc=3 inst='+' ptr=1 cell[1]=0 state=paused"));
    assert!(stdout.contains("next pc=5 inst='.'"));
    assert!(stdout.contains("inst pc=5 op='.'"));
}

#[test]
fn sanei_debugger_supports_info_watchpoints_and_set() {
    let path = env::temp_dir().join(format!("sane-debug-watch-{}.bf", std::process::id()));
    fs::write(&path, "++").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"info\nwatch 0\nwatchpoints\nunwatch 0\nwatchpoints\nwatch 0\ns\nset 0 7\nx/d 0\nq\n",
        )
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("breakpoints=0 watchpoints=0"));
    assert!(stdout.contains("watchpoint cell=0 value=0"));
    assert!(stdout.contains("watchpoints deleted 1"));
    assert!(stdout.contains("no watchpoints"));
    assert!(stdout.contains("watchpoint cell=0 old=0 new=1"));
    assert!(stdout.contains("cell[0]=7"));
    assert!(stdout.contains("0 7"));
}

#[test]
fn sanei_debugger_repeats_last_command_on_empty_line() {
    let path = env::temp_dir().join(format!("sane-debug-repeat-{}.bf", std::process::id()));
    fs::write(&path, "+++.").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"s\n\npc\nx/d 0\nq\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pc=1 inst='+' ptr=0 cell[0]=1 state=paused"));
    assert!(stdout.contains("pc=2 inst='+' ptr=0 cell[0]=2 state=paused"));
    assert!(stdout.contains("pc=2"));
    assert!(stdout.contains("0 2"));
}

#[test]
fn sanei_debugger_restart_uses_gdb_style_r_command() {
    let path = env::temp_dir().join(format!("sane-debug-restart-{}.bf", std::process::id()));
    fs::write(&path, "++").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"s 2\nr\npc\nx/d 0\nq\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("restarted"));
    assert!(stdout.contains("pc=0"));
    assert!(stdout.contains("0 0"));
}

#[test]
fn sanei_debugger_reads_sane_symbols() {
    let path = env::temp_dir().join(format!("sane-debug-symbols-{}.bf", std::process::id()));
    fs::write(
        &path,
        "SANE SYMBOLS\n\
         x CELL 8\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 13 TO 14\n\
         END SANE SYMBOLS\n\
         +",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"symbols\n\
              symbol arr\n\
              set x 9\n\
              set arr[1] 7\n\
              x/d x\n\
              x/2xb arr\n\
              x/d arr[1]\n\
              x/d arr+1\n\
              x/d x\n\
              q\n",
        )
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("x cell 8"));
    assert!(stdout.contains("arr array base 9 len 2 data 13 to 14"));
    assert!(stdout.contains("cell[8]=9"));
    assert!(stdout.contains("cell[14]=7"));
    assert!(stdout.contains("13 0x00"));
    assert!(stdout.contains("14 0x07"));
    assert!(stdout.contains("14 7"));
    assert!(stdout.contains("8 9"));
}

#[test]
fn sanei_debugger_watches_entire_array_symbol() {
    let path = env::temp_dir().join(format!("sane-debug-array-watch-{}.bf", std::process::id()));
    fs::write(
        &path,
        "SANE SYMBOLS\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 0 TO 1\n\
         END SANE SYMBOLS\n\
         >+",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"watch arr\nwatchpoints\nc\nq\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("watchpoint cell=0 value=0"));
    assert!(stdout.contains("watchpoint cell=1 value=0"));
    assert!(stdout.contains("watchpoint cell=1 old=0 new=1"));
}

#[test]
fn sanei_debugger_unwatches_entire_array_symbol() {
    let path = env::temp_dir().join(format!(
        "sane-debug-array-unwatch-{}.bf",
        std::process::id()
    ));
    fs::write(
        &path,
        "SANE SYMBOLS\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 0 TO 1\n\
         END SANE SYMBOLS\n\
         >+",
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_sanei"))
        .arg("-d")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"watch arr\nunwatch arr\nwatchpoints\nc\nq\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("watchpoints deleted 2"));
    assert!(stdout.contains("no watchpoints"));
    assert!(!stdout.contains("old=0 new=1"));
}
