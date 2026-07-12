use std::env;
use std::fs;
use std::io::{self, Read};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| arg == "-h" || arg == "--help")
    {
        println!("{}", usage());
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|arg| arg == "-V" || arg == "--version")
    {
        println!("sanec {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let input = if args.first().is_some_and(|arg| arg != "-o") {
        Some(args.remove(0))
    } else {
        None
    };

    let output = match args.as_slice() {
        [] => None,
        [flag, path] if flag == "-o" => Some(path.clone()),
        [arg, ..] => return Err(format!("unexpected argument `{arg}`\n\n{}", usage())),
    };

    let (src, path) = if let Some(path) = input {
        (
            fs::read_to_string(&path).map_err(|e| format!("failed to read `{path}`: {e}"))?,
            path,
        )
    } else {
        let mut src = String::new();
        io::stdin()
            .read_to_string(&mut src)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        (src, "<stdin>".to_string())
    };

    let bf = sane::compile_source_with_path(&src, &path)?;

    if let Some(path) = output {
        fs::write(&path, bf).map_err(|e| format!("failed to write `{path}`: {e}"))?;
    } else {
        print!("{bf}");
    }

    Ok(())
}

fn usage() -> String {
    "usage:\n  sanec [source.sn] [-o out.bf]\n  sanec --help\n  sanec --version".to_string()
}
