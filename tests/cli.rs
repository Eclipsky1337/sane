mod common;

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::run_bf;

static TEMP_FILE_ID: AtomicUsize = AtomicUsize::new(0);

fn run_sanec(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sanec"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn run_debugger(name: &str, program: &str, commands: &str) -> String {
    let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!("sane-{name}-{}-{id}.bf", std::process::id()));
    fs::write(&path, program).unwrap();

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
        .write_all(commands.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    fs::remove_file(path).unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

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
    assert!(
        help.contains("-b <backend>    Select backend: auto, structured, or pc (default: auto)")
    );
}

#[test]
fn sanec_can_compile_with_pc_backend() {
    let output = run_sanec(&["-b", "pc"], "let x = 3; while x { put '0' + x; x -= 1; }");
    assert!(output.status.success());
    assert_eq!(
        run_bf(&String::from_utf8(output.stdout).unwrap(), &[]),
        b"321"
    );
}

#[test]
fn sanec_automatically_selects_pc_for_functions() {
    let output = run_sanec(&[], "fn value() -> byte { return 42; } println value();");
    assert!(output.status.success());
    assert_eq!(
        run_bf(&String::from_utf8(output.stdout).unwrap(), &[]),
        b"42\n"
    );
}

#[test]
fn sanec_automatically_selects_pc_for_void_functions() {
    let output = run_sanec(&[], "fn show() { puts \"ok\\n\"; } show();");
    assert!(output.status.success());
    assert_eq!(
        run_bf(&String::from_utf8(output.stdout).unwrap(), &[]),
        b"ok\n"
    );
}

#[test]
fn sanec_can_explicitly_select_auto_backend() {
    let output = run_sanec(&["-b", "auto"], "put 'A';");
    assert!(output.status.success());
    assert_eq!(
        run_bf(&String::from_utf8(output.stdout).unwrap(), &[]),
        b"A"
    );
}

#[test]
fn sanec_forced_structured_backend_rejects_functions() {
    let output = run_sanec(
        &["-b", "structured"],
        "fn value() -> byte { return 42; } println value();",
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("functions require the pc backend"),
        "{stderr}"
    );
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
    let output = run_sanec(&["-s"], "let x = 'A'; let msg: byte[2] = \"hi\"; put x;");
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
fn sanec_can_annotate_empty_inferred_string_arrays() {
    let output = run_sanec(&["-s"], "let empty = \"\"; puts \"ok\";");
    assert!(output.status.success());

    let bf = String::from_utf8(output.stdout).unwrap();
    assert!(bf.contains("empty ARRAY BASE 8 LEN 0 DATA CELLS 12 TO 11\n"));
    assert_eq!(run_bf(&bf, &[]), b"ok");
}

#[test]
fn sanei_debugger_supports_stepping_breakpoints_and_tape_inspection() {
    let stdout = run_debugger(
        "debug",
        "++>+<.",
        "s 2\npc\nb 5\nbreakpoints\nc\nx/2d 0\nx/xw 0\nq\n",
    );

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
    let stdout = run_debugger("debug-next", "++>+<.", "next >\npc\ns\nnext .\ninst\nq\n");

    assert!(stdout.contains("next pc=2 inst='>'"));
    assert!(stdout.contains("pc=2"));
    assert!(stdout.contains("pc=3 inst='+' ptr=1 cell[1]=0 state=paused"));
    assert!(stdout.contains("next pc=5 inst='.'"));
    assert!(stdout.contains("inst pc=5 op='.'"));
}

#[test]
fn sanei_debugger_supports_info_watchpoints_and_set() {
    let stdout = run_debugger(
        "debug-watch",
        "++",
        "info\nwatch 0\nwatchpoints\nunwatch 0\nwatchpoints\nwatch 0\ns\nset 0 7\nx/d 0\nq\n",
    );

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
    let stdout = run_debugger("debug-repeat", "+++.", "s\n\npc\nx/d 0\nq\n");

    assert!(stdout.contains("pc=1 inst='+' ptr=0 cell[0]=1 state=paused"));
    assert!(stdout.contains("pc=2 inst='+' ptr=0 cell[0]=2 state=paused"));
    assert!(stdout.contains("pc=2"));
    assert!(stdout.contains("0 2"));
}

#[test]
fn sanei_debugger_restart_uses_gdb_style_r_command() {
    let stdout = run_debugger("debug-restart", "++", "s 2\nr\npc\nx/d 0\nq\n");

    assert!(stdout.contains("restarted"));
    assert!(stdout.contains("pc=0"));
    assert!(stdout.contains("0 0"));
}

#[test]
fn sanei_debugger_reads_sane_symbols() {
    let stdout = run_debugger(
        "debug-symbols",
        "SANE SYMBOLS\n\
         x CELL 8\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 13 TO 14\n\
         END SANE SYMBOLS\n\
         +",
        "symbols\nsymbol arr\nset x 9\nset arr[1] 7\nx/d x\nx/2xb arr\nx/d arr[1]\nx/d arr+1\nx/d x\nq\n",
    );

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
    let stdout = run_debugger(
        "debug-array-watch",
        "SANE SYMBOLS\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 0 TO 1\n\
         END SANE SYMBOLS\n\
         >+",
        "watch arr\nwatchpoints\nc\nq\n",
    );

    assert!(stdout.contains("watchpoint cell=0 value=0"));
    assert!(stdout.contains("watchpoint cell=1 value=0"));
    assert!(stdout.contains("watchpoint cell=1 old=0 new=1"));
}

#[test]
fn sanei_debugger_unwatches_entire_array_symbol() {
    let stdout = run_debugger(
        "debug-array-unwatch",
        "SANE SYMBOLS\n\
         arr ARRAY BASE 9 LEN 2 DATA CELLS 0 TO 1\n\
         END SANE SYMBOLS\n\
         >+",
        "watch arr\nunwatch arr\nwatchpoints\nc\nq\n",
    );

    assert!(stdout.contains("watchpoints deleted 2"));
    assert!(stdout.contains("no watchpoints"));
    assert!(!stdout.contains("old=0 new=1"));
}
