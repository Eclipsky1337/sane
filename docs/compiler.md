# Compiler

`sanec` compiles Sane source into Brainfuck.

## Usage

```text
Usage: sanec [source.sn] [-o out.bf] [-s] [-c on|off]

Options:
  source.sn       Read Sane source from file, or stdin if omitted
  -o <file>       Write Brainfuck output to <file>
  -s              Add BF-safe symbol table comments
  -c <on|off>     Compress repeated + - < > commands (default: on)
  -h, --help      Show this help text
  -V, --version   Show compiler version
```

## Input And Output

With a source path, `sanec` reads that file:

```sh
sanec program.sn
```

Without a source path, it reads Sane source from standard input:

```sh
printf "put 'A';" | sanec
```

Generated Brainfuck is written to standard output unless `-o` specifies a
file:

```sh
sanec program.sn -o program.bf
```

The compiler resolves `const` declarations without allocating tape cells.
Constants can define byte, int, and fixed expressions. Array lengths may also
be inferred from list and string initializers; see the
[Language Reference](language.md) for syntax and scope rules.

Formatted output such as `print "step {}: {:d} ({:c})\n", step, value, mark;`
is validated at compile time and lowered to literal, decimal, and raw-byte
output operations.

Formatted input such as `input("{} {:c}", value, mark)` is also parsed and
validated at compile time. The generated program performs exact literal
matching, checked byte or signed-int decimal conversion, raw-byte fields, and
returns the successful conversion count. Runtime format strings are not
supported.

## Backend

All programs, with or without functions, lower to basic-block IR and use the
PC backend. It emits a Brainfuck dispatcher loop and supports byte, int, fixed,
and void functions, expression and statement calls, and returns:

```sh
sanec examples/Collatz.sn -o collatz.bf
```

The former `-b` backend-selection option is no longer supported.
The PC backend supports at most 256 basic blocks. This baseline keeps every
basic block produced by lowering and executes each block in its own dispatcher
match.

Runs of three or more `+`, `-`, `<`, or `>` commands are written with a decimal
repeat count by default: `>>>` becomes `3>`, while `>>` and `++` stay unchanged.
`.`, `,`, `[`, and `]` are never counted. `sanec -c off` leaves those runs
uncompressed. `sanei` accepts either form. Ordinary Brainfuck interpreters
ignore the digits, so they execute a compressed run as one command.

## Symbol Annotations

`-s` prepends a public symbol table to the generated Brainfuck:

```sh
sanec program.sn -o program.bf -s
```

The annotation includes temporary, scratch, control, scalar, and array cell
locations. It avoids all eight Brainfuck instruction characters, so conforming
interpreters ignore it as comments.

Example shape:

```text
SANE SYMBOLS
TEMP CELLS 0 TO 7
SCRATCH BASE 16
CONTROL BASE 28
x CELL 8
data ARRAY BASE 9 LEN 3 DATA CELLS 13 TO 15
END SANE SYMBOLS
```

The annotations are ignored by Brainfuck interpreters and can be consumed by
external tools.

## Diagnostics

Lexer, parser, and semantic errors include the source path, line, column, and a
caret:

```text
expected Semi, found Put
  --> bad.sn:2:1
   |
 2 | put x;
   | ^
```

Compiler failures are written to standard error and return a non-zero exit
status.

## Examples

```sh
sanec examples/toy_aes_round.sn -o toy.bf -s
sanec examples/Collatz.sn -o collatz.bf
```
