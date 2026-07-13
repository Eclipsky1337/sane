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

    let mut input = None;
    let mut output = None;
    let mut annotate_symbols = false;
    let mut backend = Backend::Structured;

    while let Some(arg) = args.first().cloned() {
        args.remove(0);
        match arg.as_str() {
            "-o" => {
                let Some(path) = args.first().cloned() else {
                    return Err(format!("missing output path after `-o`\n\n{}", usage()));
                };
                args.remove(0);
                output = Some(path);
            }
            "-s" => annotate_symbols = true,
            "-b" => {
                let Some(name) = args.first().cloned() else {
                    return Err(format!("missing backend after `-b`\n\n{}", usage()));
                };
                args.remove(0);
                backend = Backend::parse(&name)?;
            }
            _ if arg.starts_with('-') => {
                return Err(format!("unexpected argument `{arg}`\n\n{}", usage()));
            }
            _ if input.is_none() => input = Some(arg),
            _ => return Err(format!("unexpected argument `{arg}`\n\n{}", usage())),
        }
    }

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

    let mut bf = compile_source(&src, &path, backend)?;
    if annotate_symbols {
        let symbols = resolve_symbols(&src, &path)?;
        bf = format!("{}\n{bf}", format_symbol_annotation(&symbols));
    }

    if let Some(path) = output {
        fs::write(&path, bf).map_err(|e| format!("failed to write `{path}`: {e}"))?;
    } else {
        print!("{bf}");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Backend {
    Structured,
    Pc,
}

impl Backend {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "structured" => Ok(Self::Structured),
            "pc" => Ok(Self::Pc),
            _ => Err(format!(
                "unknown backend `{name}`; expected `structured` or `pc`"
            )),
        }
    }
}

fn compile_source(src: &str, path: &str, backend: Backend) -> Result<String, String> {
    match backend {
        Backend::Structured => sane::compile_source_with_path(src, path),
        Backend::Pc => {
            let tokens = sane::lexer::lex(src).map_err(|err| err.render(path, src))?;
            let mut parser = sane::parser::Parser::new(tokens);
            let program = parser
                .parse_program()
                .map_err(|err| err.render(path, src))?;
            let program = sane::sema::resolve(&program).map_err(|err| err.render(path, src))?;
            sane::bf::compile_pc(&program)
        }
    }
}

fn resolve_symbols(src: &str, path: &str) -> Result<sane::sema::Symbols, String> {
    let tokens = sane::lexer::lex(src).map_err(|err| err.render(path, src))?;
    let mut parser = sane::parser::Parser::new(tokens);
    let program = parser
        .parse_program()
        .map_err(|err| err.render(path, src))?;
    Ok(sane::sema::resolve(&program)
        .map_err(|err| err.render(path, src))?
        .symbols)
}

fn format_symbol_annotation(symbols: &sane::sema::Symbols) -> String {
    let mut lines = Vec::new();
    lines.push("SANE SYMBOLS".to_string());
    lines.push(format!("TEMP CELLS 0 TO {}", sane::sema::TEMP_COUNT - 1));
    lines.push(format!("SCRATCH BASE {}", symbols.scratch_base()));
    lines.push(format!("CONTROL BASE {}", symbols.control_base()));

    for entry in symbols.entries() {
        match entry {
            sane::sema::SymbolInfo::Scalar { name, cell } => {
                lines.push(format!("{name} CELL {cell}"));
            }
            sane::sema::SymbolInfo::Array { name, base, len } => {
                let first = base + 4;
                let last = first + len - 1;
                lines.push(format!(
                    "{name} ARRAY BASE {base} LEN {len} DATA CELLS {first} TO {last}"
                ));
            }
        }
    }

    lines.push("END SANE SYMBOLS".to_string());
    lines.join("\n")
}

fn usage() -> String {
    format!(
        "\
Usage: sanec [source.sn] [-o out.bf] [-s] [-b backend]

Options:
  source.sn       Read Sane source from file, or stdin if omitted
  -o <file>       Write Brainfuck output to <file>
  -s              Add BF-safe symbol table comments
  -b <backend>    Select backend: structured or pc
  -h, --help      Show this help text
  -V, --version   Show compiler version
"
    )
}
