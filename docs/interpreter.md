# Interpreter And Debugger

`sanei` executes Brainfuck programs and includes a raw-instruction debugger.

## Usage

```text
Usage: sanei [-d] <program.bf>

Options:
  program.bf      Run Brainfuck program from <program.bf>
  -d              Run program in interactive debug mode
  -h, --help      Show this help text
  -V, --version   Show interpreter version

Notes:
  Non-Brainfuck characters are ignored
  Program input is read from stdin
```

Run a program:

```sh
sanei program.bf
```

Program input is read from standard input:

```sh
printf 1230 | sanei luhn4.bf
```

EOF stores zero in the current cell. The interpreter uses a 30,000-cell tape
with wrapping 8-bit cells. Output is flushed after every Brainfuck `.` so
interactive prompts are visible before the program requests input.

## Parsing And Optimization

`sanei` ignores non-Brainfuck characters, validates brackets once, and parses
the program into optimized operations before execution.

| Brainfuck pattern | Parsed operation |
| --- | --- |
| `+++----` | Combined add/subtract |
| `>>>>><` | Combined pointer movement |
| `[-]` or `[+]` | Clear current cell |
| `[->++<]` | Multiply-add transfer loop |

## Debug Mode

Start the debugger with `-d`:

```sh
sanei -d program.bf
```

The debugger operates on raw Brainfuck instructions rather than optimized
interpreter operations. Pressing Enter repeats the previous command. During
`continue`, Ctrl-C pauses execution and returns to the prompt.

### Execution Commands

```text
r                  Restart program
c                  Continue until breakpoint, Ctrl-C, or halt
s [n]              Step n raw BF instructions
next <inst>        Continue until next matching BF instruction
q                  Exit debugger
```

### Breakpoints And Watchpoints

```text
b <pc>             Set breakpoint at instruction index
delete <pc>        Delete breakpoint
breakpoints        List breakpoints
watch <addr|sym>   Stop when a cell changes
unwatch <addr|sym> Remove watchpoint by cell or symbol
watchpoints        List watchpoints
```

Watching an array symbol watches its complete data range. An indexed array
symbol watches one element.

### Program State

```text
info               Show pc, instruction, pointer, and breakpoint counts
pc                 Show current instruction index
inst               Show current instruction
code [n]           Show instructions around pc
set <addr|sym> <v> Set one tape cell
x[/FMT] <addr|sym> Examine tape
symbols            Show symbols embedded by sanec -s
symbol <name>      Show one symbol
```

### Examine Formats

`x` follows the GDB-style `x/COUNTFORMAT` shape:

```text
x/10d 0       show 10 decimal bytes from cell 0
x/16xb state  show 16 hexadecimal bytes from state
x/4xw data    show 4 hexadecimal 4-byte words
x/c state[3]  show one cell as a character
```

Supported value formats:

| Code | Meaning |
| --- | --- |
| `d` | Decimal |
| `x` | Hexadecimal |
| `c` | Character |

Supported element sizes:

| Code | Size |
| --- | --- |
| `b` | 1 byte |
| `h` | 2 bytes |
| `w` | 4 bytes |
| `g` | 8 bytes |

Multi-cell values are displayed little-endian.

### Symbol Addresses

Brainfuck generated with `sanec -s` supports symbolic debugger addresses:

```text
x/d counter
x/16xb state
x/d state[3]
x/d state+3
set state[3] 65
watch state
```

Scalar symbols address one cell. Array symbols expose their data cells rather
than their four metadata cells.
