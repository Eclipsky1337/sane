use std::env;
use std::fs;
use std::io::{self, Write};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{}", usage());
            Ok(())
        }
        [flag] if flag == "-V" || flag == "--version" => {
            println!("sanei {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        [path] => run_bf(path),
        _ => Err(usage()),
    }
}

fn run_bf(path: &str) -> Result<(), String> {
    let src = fs::read_to_string(path).map_err(|e| format!("failed to read `{path}`: {e}"))?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    sane::interpreter::run_source(&src, stdin.lock(), stdout.lock())?;
    stdout
        .lock()
        .flush()
        .map_err(|e| format!("failed to flush stdout: {e}"))?;
    Ok(())
}

fn usage() -> String {
    format!(
        "\
Usage: sanei <program.bf>

Options:
  program.bf      Run Brainfuck program from <program.bf>
  -h, --help      Show this help text
  -V, --version   Show interpreter version

Notes:
  Non-Brainfuck characters are ignored
  Program input is read from stdin
"
    )
}
