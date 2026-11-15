# Interpreter

`sanei` executes Brainfuck programs.

## Usage

```text
Usage: sanei <program.bf>

Options:
  program.bf      Run Brainfuck program from <program.bf>
  -h, --help      Show this help text
  -V, --version   Show interpreter version

Notes:
  A decimal count before + - < > repeats that command, as in 3>
  Other non-Brainfuck characters are ignored
  Program input is read from stdin
```

Run a program:

```sh
sanei program.bf
```

Program input is read from standard input:

```sh
printf "7\n" | sanei collatz.bf
```

EOF stores zero in the current cell. The interpreter uses a 30,000-cell tape
with wrapping 8-bit cells. Output is flushed after every Brainfuck `.` so
interactive prompts are visible before the program requests input.

## Parsing And Execution

`sanei` accepts a decimal repeat count immediately before `+`, `-`, `<`, or
`>`. `3>` is three `>` commands, and `10+` is ten `+` commands. A count of zero
repeats nothing. Other non-Brainfuck characters are ignored.

The parser first builds a small nested form so it can inspect loop bodies, then
flattens it into one instruction stream. The runtime executes that stream with a
program counter; loops use absolute `JumpIfZero` / `JumpIfNonZero` targets
instead of recursive body calls.

| Brainfuck pattern | Bytecode operation |
| --- | --- |
| `+++--` or `3+2-` | Combined add/subtract |
| `+++>+++<` | `Block` of fused adds and pointer moves |
| `[-]`, `[+]`, `[[-]]` | `Set(0)` |
| `[-]+++` | `Set(3)` |
| `[->++<]`, `[->+>+<<]` | `AddMul` |
| `[>]`, `[<]`, `[>>]` | `ScanZero` |
| Other `[ body ]` | `JumpIfZero` + body + `JumpIfNonZero` |

`Block` checks the minimum and maximum pointer positions once, then applies all
cell edits. `AddMul` applies `value * coefficient` to the target cells and
clears the origin; it checks the body's intermediate pointer positions even
though they are not replayed. `ScanZero` moves until it finds a zero cell, and
reports an error if the tape boundary is crossed first.

`Block`, `AddMul`, and `ScanZero` preserve Brainfuck's pointer-bounds errors,
including intermediate moves that leave and re-enter the tape. Normal execution
does not collect instruction counters; the benchmark path enables them with a
separate statistics entry point. Array fast paths and native operations are not
part of this interpreter yet.
