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

Adjacent add/subtract commands are merged into one operation, and adjacent
pointer moves are merged the same way. A loop whose body is exactly `[-]` or
`[+]` clears the current cell without executing the loop body.

| Brainfuck pattern | Parsed operation |
| --- | --- |
| `+++--` or `3+2-` | Combined add/subtract |
| `>>>>><` or `5>2<` | Combined pointer movement |
| `[-]` or `[+]` | Clear current cell |

All other loops run as ordinary Brainfuck loops. There is no block fusion,
array fast path, or native operation shortcut.
