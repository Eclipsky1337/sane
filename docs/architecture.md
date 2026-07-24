# Architecture

Sane is split into a compiler library, two command-line binaries, and an
optimized Brainfuck runtime.

## Pipeline

```mermaid
flowchart LR
    subgraph compiler["sanec compiler"]
        direction TB
        source["Sane source<br/><code>.sn</code>"]
        lexer["lexer.rs<br/>tokens"]
        parser["parser.rs<br/>AST"]
        sema["sema.rs<br/>symbols + tape layout"]
        codegen["bf.rs<br/>Brainfuck codegen"]
        output["Brainfuck output<br/><code>.bf</code>"]

        source --> lexer --> parser --> sema --> codegen --> output
    end

    subgraph runtime["sanei interpreter"]
        direction TB
        input["Brainfuck input<br/><code>.bf</code>"]
        parse["interpreter.rs<br/>parse + optimize"]
        execute["30,000-cell runtime"]
        result["program output"]

        input --> parse --> execute --> result
    end

    compiler --> runtime
```

## Compiler Stages

1. `lexer.rs` converts source text into tokens and validates literals.
2. `parser.rs` builds the syntax tree for declarations, expressions, and control flow.
3. `sema.rs` resolves scopes, validates symbols, and assigns tape cells.
4. `bf.rs` lowers resolved statements and expressions into Brainfuck microcode.
5. The final Brainfuck pass removes redundant adjacent clear loops.

Diagnostics retain source spans through parsing and semantic analysis so errors
can point back to the original line and column.

## Tape Layout

Generated programs use a static tape layout:

```text
cell 0..7                  compiler temporaries
cell 8..scratch_base-1     source variables and arrays
cell scratch_base..+11     primitive scratch cells
cell control_base..        control-flow guard cells
```

### Temporaries

Cells `0..7` are short-lived expression temporaries. Arithmetic and copy
microcode may overwrite them.

### Source Storage

Each scalar occupies one cell. Lexical-scope cells return to a free-list after
the scope ends, allowing later non-overlapping variables to reuse storage.

Arrays occupy four metadata cells followed by their data:

```text
base + 0     space
base + 1     index1
base + 2     index2
base + 3     data transfer cell
base + 4..   array elements
```

Constant array accesses compile directly to a known data cell. Dynamic accesses
use the metadata cells and array traversal microcode.

### Scratch And Control Cells

The 12 primitive scratch cells support comparisons, division, bitwise
operations, shifts, and decimal printing. Control cells are allocated in a
stack-like manner while emitting nested conditions and loops.

## Control Flow Lowering

Brainfuck has loops but no jumps. Structured Sane control flow is implemented
with guard cells:

- `if` evaluates a boolean guard and executes its body at most once.
- `if/else` uses a second flag to select the else branch.
- loops reevaluate their condition through guard and recheck cells.
- `break` clears the active loop controls.
- `continue` clears the remaining-body guard and preserves the next iteration.

## Interpreter Runtime

The normal interpreter parses Brainfuck into higher-level operations before
execution. The debugger intentionally uses raw instructions so breakpoints and
single stepping correspond directly to Brainfuck program counters.

Symbol comments produced by `sanec -s` are ignored during execution but parsed
by the debugger for tape inspection.
