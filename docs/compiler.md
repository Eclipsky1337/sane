# Compiler

`sanec` compiles Sane source into Brainfuck.

## Usage

```text
Usage: sanec [source.sn] [-o out.bf] [-s] [-b backend]

Options:
  source.sn       Read Sane source from file, or stdin if omitted
  -o <file>       Write Brainfuck output to <file>
  -s              Add BF-safe symbol table comments
  -b <backend>    Select backend: auto, structured, or pc (default: auto)
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

## Backends

The default `auto` mode selects the structured backend for programs without
functions and the PC backend for programs containing functions.

The `structured` backend lowers control flow directly to Brainfuck loops and
guard cells. It generally produces smaller and faster programs for language
features that do not require functions:

```sh
sanec -b structured program.sn -o program.bf
```

The experimental `pc` backend lowers the program to basic-block IR and emits a
Brainfuck dispatcher loop. It supports byte and void functions, expression and
statement calls, and returns:

```sh
sanec -b pc examples/Collatz.sn -o collatz.bf
```

Use `-b structured` or `-b pc` to force a backend. Forcing the structured
backend for a program containing functions reports an actionable error. Both
backends accept the same non-function syntax.

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
SCRATCH BASE 12
CONTROL BASE 24
x CELL 8
data ARRAY BASE 9 LEN 3 DATA CELLS 13 TO 15
END SANE SYMBOLS
```

These symbols can be inspected by the `sanei` debugger. See
[Interpreter And Debugger](interpreter.md).

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
sanec examples/luhn4.sn -o luhn4.bf
sanec examples/toy_aes_round.sn -o toy.bf -s
sanec examples/Collatz.sn -o collatz.bf
```
