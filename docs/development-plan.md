# Post-1.1 Development Plan

This document is the handoff context for the next Codex session. Work should
continue on the dedicated `dev` branch. Do not develop directly on `main`.

## Progress

- [x] Phase 1: P2 language usability
  - [x] automatic backend selection
  - [x] void functions and call statements
  - [x] inferred array declarations
  - [x] compile-time constants
- [ ] Phase 2: P1 PC measurement and optimization
- [ ] Phase 3: P3 item 13 debug symbols

Phase 1 passed `cargo fmt -- --check`, `cargo test --locked`,
`git diff --check`, and the documented AES and Collatz example checks.

## Repository State

- Project: Sane
- Compiler: `sanec`
- Interpreter/debugger: `sanei`
- Current release: `v1.1.0`
- Current development branch: `dev`
- `main` and `v1.1.0` point to `584f1dc`; `dev` contains post-1.1 work
- Package version: `1.1.0`
- Commit messages use prefixes such as `feat:`, `fix:`, and `doc:`
- The full test suite currently contains 106 tests

Before starting work:

```sh
git switch dev
git fetch origin
git rebase origin/main
cargo fmt -- --check
cargo test --locked
```

Do not rewrite or delete `v1.1.0`. Keep `dev` rebased on `main` and avoid merge
commits when updating the development branch.

## Current Language

Sane compiles byte-oriented source to 8-bit Brainfuck.

Implemented language features:

- wrapping `byte` values
- lexical scalar variables
- fixed-size byte arrays
- explicit and inferred list/ASCII string array initializers
- lexically scoped compile-time byte constants
- arithmetic, comparisons, logical operators, bitwise operators, and shifts
- `if`, `while`, `loop`, `for`, `break`, and `continue`
- byte and decimal input/output statements
- byte and void functions, parameters, returns, expression calls, and call
  statements

Current function syntax:

```sane
fn add(a: byte, b: byte) -> byte {
    return a + b;
}

let result = add(20, 22);
```

Current important limits:

- automatic backend selection uses PC for programs containing functions
- forcing `sanec -b structured` rejects functions
- function parameters and value-returning functions are byte-only
- recursive call graphs are rejected
- each function uses one statically allocated frame
- maximum static call depth is 16
- the PC backend supports at most 256 basic blocks
- dynamic array indexes are not bounds checked
- array elements are byte-only
- there are no modules, pointers, or wider integer types

## Compiler Architecture

Relevant pipeline:

```text
source
  -> lexer.rs
  -> parser.rs / AST
  -> sema.rs / resolved program and tape allocation
  -> structured backend in bf.rs
       or
     ir.rs / basic blocks
       -> PC backend in bf.rs
  -> Brainfuck
```

Important files:

| File | Responsibility |
| --- | --- |
| `src/lexer.rs` | Tokens and literals |
| `src/ast.rs` | Parsed function, statement, and expression forms |
| `src/parser.rs` | Source grammar |
| `src/sema.rs` | Name resolution, call graph checks, cells, frames, and spills |
| `src/ir.rs` | PC-backend basic-block lowering |
| `src/bf.rs` | Structured codegen, PC dispatcher, calls, and BF microcode |
| `src/bin/sanec.rs` | Compiler CLI and backend selection |
| `src/interpreter.rs` | Optimized Brainfuck execution |
| `src/debug.rs` | Raw-instruction debugger and symbol support |

Backend behavior:

- `structured` directly lowers control flow to BF loops and guard cells.
- `pc` lowers to basic blocks with `Jump`, `Branch`, `Call`, `Return`, or
  `Halt` terminators.
- The PC runtime uses a byte-sized `pc`, `running`, dispatch scratch cells,
  `rv`, call depth, and 16 return-PC cells.
- The current dispatcher linearly compares the selected PC with every block on
  every dispatch.

## Measured PC Backend State

The following measurements were taken from the current 1.1 implementation.
They are reference values, not exact assertions for future tests.

Representative inputs:

```text
examples/luhn4.sn          1230
examples/toy_aes_round.sn  ABCDEFGHIJKLMNOP
examples/Collatz.sn        7\n
```

Static IR:

| Program | Blocks | Ordinary ops | Empty-op blocks | Trivial jumps | Unreachable |
| --- | ---: | ---: | ---: | ---: | ---: |
| `luhn4` | 37 | 30 | 20 | 11 | 0 |
| `toy_aes_round` | 46 | 112 | 18 | 2 | 0 |
| `Collatz` | 26 | 13 | 19 | 11 | 6 |

Dynamic PC execution:

| Program | Dispatches | Used blocks | Empty-op dispatches | Sequential transitions |
| --- | ---: | ---: | ---: | ---: |
| `luhn4` | 25 | 25/37 | 80.0% | 33.3% |
| `toy_aes_round` | 629 | 44/46 | 52.9% | 48.9% |
| `Collatz` | 171 | 19/26 | 68.4% | 30.0% |

Generated BF instruction counts:

| Program | PC backend | Structured backend |
| --- | ---: | ---: |
| `luhn4` | 29,245 | 10,647 |
| `toy_aes_round` | 496,755 | 554,894 |
| `Collatz` | 65,074 | unavailable because it uses functions |

Interpret "empty-op block" carefully: such a block may still perform a
`Branch`, `Call`, or `Return`. Only an empty block with an unconditional jump is
trivially removable. The measurements show that dispatcher overhead is real,
but language usability work has priority in the next session.

## Required Work Order

The requested order is:

1. P2: language usability
2. P1: PC-backend measurement and optimization
3. P3 item 13 only: function and PC debugging symbols

Do not start wider integers, pointers, recursion, modules, `switch`, or a
multi-cell PC during this plan.

## Phase 1: P2 Language Usability

Implement these items in order, with a focused commit for each coherent change.

### 1. Automatic Backend Selection

Change the normal compiler default from `structured` to `auto`.

Required behavior:

- no functions: select the structured backend
- one or more functions: select the PC backend
- `-b structured`: force structured and retain a clear function error
- `-b pc`: force PC
- optionally accept `-b auto`, but the absence of `-b` must mean auto

The backend choice should use the resolved or parsed program, not source-text
searching.

Acceptance examples:

```sh
sanec examples/luhn4.sn -o luhn.bf
sanec examples/Collatz.sn -o collatz.bf
sanec -b structured examples/Collatz.sn
sanec -b pc examples/luhn4.sn -o luhn-pc.bf
```

The first two commands must compile. The third must fail with an actionable
diagnostic. Update compiler help and documentation.

### 2. Void Functions And Call Statements

Add functions with no return value:

```sane
fn show(value: byte) {
    println value;
}

show(42);
```

Required syntax and semantics:

- omitted return type means void
- `return;` is valid only in a void function
- falling off the end of a void function returns normally
- `return expression;` remains required for byte-returning functions
- a byte-returning call may be used as a statement and its value is discarded
- a void call is invalid where a value expression is required
- standalone call statements must work at top level and inside functions

Keep the current calling convention where practical:

- byte functions write `rv`
- void functions do not need to write `rv`
- both forms use the same return-PC stack

Add parser, semantic, IR, PC-codegen, CLI, and language tests. Include nested
void calls and invalid return-form diagnostics.

### 3. Inferred Array Declarations

Support:

```sane
let key = [1, 2, 3, 4];
let message = "hello";
```

Retain explicit declarations for uninitialized arrays:

```sane
let buffer: byte[32];
```

Requirements:

- infer array length from list initializers
- infer byte-array length from decoded string bytes
- reject empty inferred arrays unless a clear element type can be established
- preserve existing constant-initializer validation
- keep scalar `let value = expression;` behavior unchanged

Update the AST only as much as needed; the resolved representation should still
contain a concrete array length and bytes.

### 4. Compile-Time Constants

Add top-level and block-scoped constants:

```sane
const BLOCK_SIZE = 16;
const NEWLINE = '\n';

let state: byte[BLOCK_SIZE];
```

Initial scope:

- constants evaluate at compile time
- constants do not allocate tape cells
- constant expressions may use literals, earlier visible constants, unary
  operators, and existing binary operators
- constants may be used in ordinary expressions, array lengths, and array
  initializers
- reject forward references, cycles, non-constant variables, calls, and array
  accesses
- arithmetic keeps byte wrapping behavior unless array lengths require a
  separate checked size representation

Decide and document array-length behavior before implementation. A practical
first version may require the final length to be a non-zero byte value.

### Phase 1 Completion Criteria

- all four features are documented in `docs/language.md` and
  `docs/compiler.md`
- compiler help matches actual backend options
- existing programs remain source-compatible
- examples compile without manually selecting `-b pc`
- `cargo fmt -- --check` passes
- `cargo test --locked` passes

## Phase 2: P1 PC Measurement And Optimization

Do not optimize from intuition. Add observability first and retain before/after
measurements for the three reference programs.

### 5. IR Statistics

Introduce a test-only or internal statistics helper with fields such as:

```rust
struct IrStats {
    blocks: usize,
    reachable_blocks: usize,
    empty_blocks: usize,
    trivial_jumps: usize,
    calls: usize,
    branches: usize,
}
```

Use upper-bound regression assertions rather than exact block counts.

### 6. Reproducible Benchmarks

Measure at least:

- IR block count
- generated BF instruction count
- parsed `sanei` operation count if accessible
- PC dispatch count
- execution time or a deterministic execution-work metric

Use the same examples and inputs listed in the measured-state section.
Benchmarks should not be part of the normal correctness test suite if timing
would make CI flaky.

### 7. Superblocks And Fallthrough

After measurement infrastructure exists, reduce redispatch on linear paths.

Preferred approach:

- form a superblock across unique-successor and unique-predecessor edges
- stop at branches, calls, returns, shared targets, and function entries
- execute the superblock after one dispatcher match
- avoid duplicating IR operations to prevent BF code-size growth

The current sequential-transition measurements indicate that
`toy_aes_round.sn` is the most important case.

Minimal CFG support may be implemented here if required. The previously
proposed full P0 optimization series is intentionally not part of the immediate
work order. Do not silently expand this phase into a large optimizer rewrite.

### 8. Dispatcher Strategy

Measure again after superblocks. Only then evaluate alternatives to the current
linear scan:

- keep linear dispatch for small block counts
- consider grouped or hierarchical dispatch above a measured threshold
- compare BF size and execution work, not host-side intuition

Do not implement a multi-cell PC in this phase.

### Phase 2 Completion Criteria

- before/after numbers are recorded for all three reference examples
- PC optimization preserves exact program output
- no regression in structured-backend output behavior
- BF size growth, if any, is explicitly justified by execution improvement
- the 256-block diagnostic remains correct

## Phase 3: P3 Item 13 Debug Symbols

Only item 13 is in scope for this phase.

Extend BF-safe symbol annotations to describe PC programs and functions:

- function name and entry block/PC
- basic block id and source span or source line
- function parameters and local-variable cells
- call-site continuation block
- spill cells used by expression calls
- named runtime cells such as `pc`, `rv`, and call depth
- return-PC stack range

The generated annotation must continue avoiding all eight BF instruction
characters so ordinary Brainfuck interpreters treat it as comments.

Update `sanei` symbol parsing and debugger commands so users can at least:

```text
break <function>
symbol <function>
info
```

Source-level `next`, backtraces, and full `info locals` may be designed on top
of the metadata, but should not be added unless the basic symbol format is
stable and tested.

### Phase 3 Completion Criteria

- `sanec -s` emits function and block metadata for PC programs
- existing scalar and array symbols remain compatible
- raw BF execution is unchanged by annotations
- malformed symbol comments fail gracefully in `sanei`
- debugger tests cover function lookup and breakpoints by function name

## Test And Documentation Discipline

For every phase:

```sh
cargo fmt -- --check
cargo test --locked
git diff --check
```

Also run the meaningful examples:

```sh
sanec examples/toy_aes_round.sn -o /tmp/toy.bf
printf ABCDEFGHIJKLMNOP | sanei /tmp/toy.bf

sanec examples/Collatz.sn -o /tmp/collatz.bf
printf "7\n" | sanei /tmp/collatz.bf
```

Expected AES output:

```text
toy-aes:ED958E73EFAD3E04336822CC237B065A ok
```

Collatz should end at `step 16: 1` for input `7`.

Keep syntax tests independent from `examples/`. Examples are user-facing
programs, not the language coverage matrix.

## Explicitly Deferred Work

Do not include these features without a new design discussion:

- `int`, `short`, or other wider integers
- pointers
- recursion or a general dynamic call stack
- modules/imports
- `switch`
- mandatory array bounds checks
- a multi-cell PC
- a standard library

The next session should begin with Phase 1 item 1: automatic backend selection.
