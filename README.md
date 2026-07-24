# Sane

Sane is a small programming language that compiles to 8-bit Brainfuck.

- `sanec` compiles Sane source (`.sn`) to Brainfuck (`.bf`).
- `sanei` runs and debugs Brainfuck programs.

Sane 1.0 focuses on byte-oriented programs with variables, arrays, arithmetic,
I/O, and structured control flow.

## Quick Start

After placing the release binaries on `PATH`, compile and run the AES-inspired
example:

```sh
sanec examples/toy_aes_round.sn -o toy.bf
printf ABCDEFGHIJKLMNOP | sanei toy.bf
```

Expected output:

```text
toy-aes:ED958E73EFAD3E04336822CC237B065A ok
```

Compile from standard input:

```sh
printf "puts \"hello\\n\";" | sanec > hello.bf
sanei hello.bf
```

## Documentation

- [Language Reference](docs/language.md): syntax, types, operators, arrays, I/O, and control flow.
- [Compiler](docs/compiler.md): `sanec` usage, options, diagnostics, and symbol annotations.
- [Interpreter And Debugger](docs/interpreter.md): `sanei` usage, optimizations, and debugger commands.
- [Architecture](docs/architecture.md): compilation pipeline, tape layout, and implementation notes.
- [Changelog](CHANGELOG.md): release history.

## Examples

The examples are complete programs rather than syntax test cases:

| Program | Description |
| --- | --- |
| `examples/luhn4.sn` | Validates a four-digit Luhn checksum. |
| `examples/toy_aes_round.sn` | Runs an AES-inspired encrypt/decrypt round trip. |

## Development

Build the binaries and run the full test suite:

```sh
cargo build
cargo test
```

Run a development binary without installing it:

```sh
cargo run --bin sanec -- --help
cargo run --bin sanei -- --help
```

## Roadmap

- Functions with parameters, returns, and call frames.
- Additional integer widths built from multiple Brainfuck cells.
- Safer dynamic array indexing.
- Richer compile-time constants and initializer expressions.
- A module or file system for larger programs.

## License

Sane is available under the [MIT License](LICENSE).
