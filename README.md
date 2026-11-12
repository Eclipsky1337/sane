# Sane

Sane is a small programming language that compiles to 8-bit Brainfuck.

- `sanec` compiles Sane source (`.sn`) to Brainfuck (`.bf`).
- `sanei` runs Brainfuck programs.

Sane supports byte, signed 32-bit integer, and signed Q16.16 fixed-point values.
All programs use basic-block IR and the PC backend, including programs without
functions. The language also supports typed compile-time constants, inferred
arrays, and compile-time checked formatted input and output.

This branch is the unoptimized baseline used for optimization comparisons. It
keeps repeat compression, the `[-][-]` peephole, byte constant folding and
specialization, delta-encoded string output, and interpreter add/move merging
plus `[-]`/`[+]` recognition. `sanec -c off` additionally enables the constant
materialization planner.

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

Compile a function program:

```sh
sanec examples/Collatz.sn -o collatz.bf
printf "7\n" | sanei collatz.bf
```

## Documentation

- [Language Reference](docs/language.md): syntax, types, functions, arrays, I/O, and control flow.
- [Compiler](docs/compiler.md): `sanec` usage, PC backend, diagnostics, and symbol annotations.
- [Interpreter](docs/interpreter.md): `sanei` usage and command parsing.
- [Architecture](docs/architecture.md): compilation pipeline, IR, tape layout, and implementation notes.
- [32-Bit Integer Design](docs/int-design.md): `int` semantics, layout, conversions, and implementation stages.
- [Q16.16 Fixed-Point Design](docs/fixed-design.md): `fixed` representation, conversions, arithmetic, and output.
- [Changelog](CHANGELOG.md): release history.

## Examples

The examples are complete programs rather than syntax test cases:

| Program | Description |
| --- | --- |
| `examples/toy_aes_round.sn` | Runs an AES-inspired encrypt/decrypt round trip. |
| `examples/Collatz.sn` | Prints a Collatz sequence using a byte function. |
| `examples/formatted_input.sn` | Parses two signed integers with a compile-time input format. |

## Development

Build the binaries:

```sh
cargo build --locked
```

Run a development binary without installing it:

```sh
cargo run --bin sanec -- --help
cargo run --bin sanei -- --help
```

## Roadmap

- Recursive functions and richer call-frame strategies.
- Additional numeric widths, math routines, and vector types.
- Safer dynamic array indexing.
- Further PC-backend code-size and dispatch optimizations.
- A module or file system for larger programs.

## License

Sane is available under the [MIT License](LICENSE).
