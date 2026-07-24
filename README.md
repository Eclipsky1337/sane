# Sane

Sane is a small programming language that compiles to 8-bit Brainfuck.

- `sanec` compiles Sane source (`.sn`) to Brainfuck (`.bf`).
- `sanei` runs and debugs Brainfuck programs.

Sane 1.1 adds byte functions and a basic-block IR through the experimental
`pc` backend. The compiler automatically uses the structured backend for
programs without functions and the PC backend for programs with functions.

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

Compile a function program with the PC backend:

```sh
sanec examples/Collatz.sn -o collatz.bf
printf "7\n" | sanei collatz.bf
```

## Documentation

- [Language Reference](docs/language.md): syntax, types, functions, arrays, I/O, and control flow.
- [Compiler](docs/compiler.md): `sanec` usage, backends, diagnostics, and symbol annotations.
- [Interpreter And Debugger](docs/interpreter.md): `sanei` usage, optimizations, and debugger commands.
- [Architecture](docs/architecture.md): compilation pipeline, IR, tape layout, and implementation notes.
- [Changelog](CHANGELOG.md): release history.

## Examples

The examples are complete programs rather than syntax test cases:

| Program | Description |
| --- | --- |
| `examples/luhn4.sn` | Validates a four-digit Luhn checksum. |
| `examples/toy_aes_round.sn` | Runs an AES-inspired encrypt/decrypt round trip. |
| `examples/Collatz.sn` | Prints a Collatz sequence using a byte function. |

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

- Recursive functions and richer call-frame strategies.
- Additional integer widths and pointer types.
- Safer dynamic array indexing.
- Further PC-backend code-size and dispatch optimizations.
- A module or file system for larger programs.

## License

Sane is available under the [MIT License](LICENSE).
