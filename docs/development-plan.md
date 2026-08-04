# Post-1.2 Development Plan

This document is the handoff context for the next Codex session. Work should
continue on the dedicated `dev` branch. Do not develop directly on `main`.

## Progress

- [x] Phase 1: P2 language usability
  - [x] automatic backend selection
  - [x] void functions and call statements
  - [x] inferred array declarations
  - [x] compile-time constants
- [x] Phase 1.5: compile-time formatted output
- [x] Phase 1.6: typed decimal and raw-byte format placeholders
- [x] Release 1.2: version, documentation, validation, and tag
- [x] Phase 2: P1 PC measurement and optimization
  - [x] IR statistics and upper-bound regression tests
  - [x] reproducible benchmarks
  - [x] superblocks and fallthrough
  - [x] dispatcher strategy evaluation
- [ ] Phase 3: P3 item 13 debug symbols
- [x] Phase 4: 32-bit signed integers
  - [x] representation and language design
  - [x] scalar type and storage layout
  - [x] `int` syntax and 32-bit literal lexing
  - [x] literals, declarations, assignment, and casts
  - [x] addition, subtraction, equality, and conditions
  - [x] signed ordering, logical, bitwise, and shift operations
  - [x] core arithmetic and comparisons
  - [x] signed decimal output and raw byte input
  - [x] function values and typed spills
  - [x] four-cell arrays and indexing
- [x] Phase 5: compile-time formatted input
  - [x] `input` syntax, AST, and semantic validation
  - [x] typed decimal and raw-byte placeholders
  - [x] conversion-count result and deterministic failure behavior
  - [x] structured and PC backend lowering
  - [x] tests, examples, CLI documentation, and language documentation
- [x] Phase 6: Q16.16 fixed-point values
  - [x] representation and language design
  - [x] literals, declarations, assignment, and casts
  - [x] arithmetic, comparisons, conditions, and wide intermediates
  - [x] constants, functions, typed spills, and arrays
  - [x] exact decimal output
  - [x] tests, examples, and documentation
- [ ] Generated-program-size optimization backlog
  - [x] remove unreachable IR blocks
  - [x] thread jumps and remove trivial blocks
  - [x] fold branches with identical targets
  - [ ] fold constant expressions and branches
  - [x] merge identical blocks
  - [x] remove unused functions
  - [ ] expand BF peephole simplification
  - [ ] track known-zero control cells
  - [ ] replace large compile-time unrolling with runtime loops
  - [ ] specialize constant arithmetic and comparisons
  - [ ] compact dynamic wide-array access
  - [ ] evaluate shared runtime microcode
  - [ ] optimize tape layout from measured access frequency
  - [ ] reconsider dispatcher and call-level transformations

Sane 1.2 passed `cargo fmt -- --check`, `cargo test --locked`,
`cargo build --locked --release`, `git diff --check`, and the documented AES
and Collatz example checks.

## Repository State

- Project: Sane
- Compiler: `sanec`
- Interpreter/debugger: `sanei`
- Current release: `v1.2.0`
- Current development branch: `dev`
- `main` and `v1.2.0` contain the completed Phase 1 language work; `dev` adds
  typed output placeholders and the tic-tac-toe example
- Package version: `1.2.0`
- Commit messages use prefixes such as `feat:`, `fix:`, and `doc:`
- The full test suite currently contains 164 tests

Before starting work:

```sh
git switch dev
git fetch origin
git rebase origin/main
cargo fmt -- --check
cargo test --locked
```

Do not rewrite or delete published release tags. Keep `dev` rebased on `main`
and avoid merge commits when updating the development branch.

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
- compile-time checked decimal and raw-byte formatted output
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

The following measurements were taken before the 1.2 optimization phase. They
are reference values, not exact assertions for future tests.

Representative inputs:

```text
examples/toy_aes_round.sn  ABCDEFGHIJKLMNOP
examples/Collatz.sn        7\n
```

Static IR:

| Program | Blocks | Ordinary ops | Empty-op blocks | Trivial jumps | Unreachable |
| --- | ---: | ---: | ---: | ---: | ---: |
| `toy_aes_round` | 46 | 112 | 18 | 2 | 0 |
| `Collatz` | 26 | 13 | 19 | 11 | 6 |

Dynamic PC execution:

| Program | Dispatches | Used blocks | Empty-op dispatches | Sequential transitions |
| --- | ---: | ---: | ---: | ---: |
| `toy_aes_round` | 629 | 44/46 | 52.9% | 48.9% |
| `Collatz` | 171 | 19/26 | 68.4% | 30.0% |

Generated BF instruction counts:

| Program | PC backend | Structured backend |
| --- | ---: | ---: |
| `toy_aes_round` | 496,755 | 554,894 |
| `Collatz` | 65,074 | unavailable because it uses functions |

### Reproducible Measurement Baseline

The pre-optimization baseline was captured at commit `4a9de53` with:

```sh
cargo bench --locked --bench pc_backend
```

The harness compiles and executes both reference programs, verifies their exact
output, and prints a Markdown table. `Parsed ops` recursively counts optimized
`sanei` operations. `Executed ops` counts optimized operations executed by the
unprofiled program, providing a deterministic execution-work metric. A BF-safe
no-op marker in a separately generated profiling build counts dispatcher-loop
iterations without changing program output.

| Program | Blocks | Reachable | IR ops | Empty | Trivial jumps | BF instructions | Parsed ops | Dispatches | Executed ops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `toy_aes_round` | 46 | 46 | 112 | 18 | 2 | 496,755 | 34,920 | 629 | 7,525,987 |
| `Collatz` | 26 | 20 | 14 | 19 | 11 | 65,074 | 4,003 | 171 | 173,052 |

`IR ops` counts every lowered `ir::Op`, including return-value transfer
operations. This definition is slightly broader than the earlier ad hoc
`Ordinary ops` measurement.

### Superblock Results

The PC generator now groups unconditional jump chains when each target has one
predecessor and is not the program entry or a function entry. A dispatcher
match executes the entire group without duplicating IR operations.

Current measurements:

| Program | Blocks | Dispatch groups | Reachable | IR ops | Empty | Trivial jumps | BF instructions | Parsed ops | Dispatches | Executed ops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `toy_aes_round` | 46 | 46 | 46 | 112 | 18 | 2 | 496,755 | 34,920 | 629 | 7,525,987 |
| `Collatz` | 26 | 22 | 20 | 14 | 19 | 11 | 63,894 | 3,899 | 136 | 142,168 |

The AES control-flow graph has no eligible unique-predecessor jump chains, so
its measurements are unchanged. For Collatz, superblocks reduce BF size by
1.8%, dispatches by 20.5%, and deterministic executed operations by 17.8%.
This result is intentionally limited to the measured CFG transformation rather
than expanding into branch duplication or a broader optimizer.

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
sanec examples/toy_aes_round.sn -o toy.bf
sanec examples/Collatz.sn -o collatz.bf
sanec -b structured examples/Collatz.sn
sanec -b pc examples/toy_aes_round.sn -o toy-pc.bf
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
measurements for the two reference programs.

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

#### Evaluation Result

An eight-block grouped-dispatch prototype was measured after superblocks. It
computed `pc / 8`, selected a group, and then linearly matched heads inside that
group. On `toy_aes_round`, which has 46 dispatch groups, the result was:

| Strategy | BF instructions | Parsed ops | Dispatches | Executed ops |
| --- | ---: | ---: | ---: | ---: |
| linear | 496,755 | 34,920 | 629 | 7,525,987 |
| grouped prototype | 511,717 | 35,100 | 629 | 7,474,483 |

The prototype increased BF size by 3.0% and parsed operations by 0.5%, while
reducing deterministic executed operations by only 0.7% and leaving dispatch
count unchanged. `Collatz` remains below the tested grouping threshold and is
better served by the linear dispatcher after superblock formation.

The grouped prototype was therefore not retained. The current measured
programs keep linear dispatch; a hierarchical strategy should be reconsidered
only when larger representative programs justify a threshold and its code-size
cost.

### Phase 2 Completion Criteria

- before/after numbers are recorded for both reference examples
- PC optimization preserves exact program output
- no regression in structured-backend output behavior
- BF size growth, if any, is explicitly justified by execution improvement
- the 256-block diagnostic remains correct

## Generated Program Size Optimization Backlog

Apply these changes one at a time and retain measurements after every step.
The ordering starts with local, behavior-preserving cleanup and postpones
runtime-layout and calling-convention changes until simpler gains are known.

The current IR optimization is limited to superblock grouping. It does not
mutate the IR or remove blocks. The final BF optimizer currently only collapses
adjacent repeated clears such as `[-][-]` into `[-]`.

For every item below:

- record the IR and BF benchmark table before and after the change
- verify exact AES and Collatz output
- preserve structured-backend behavior
- add focused tests for the transformation
- keep the change only when its size or execution benefit is measurable

### 1. Remove Unreachable IR Blocks

Compute reachability from the program entry while following calls into function
entries and their continuation blocks. Delete unvisited blocks and remap every
`BlockId`, function entry, branch target, call continuation, and jump target.

This should remove the six unreachable blocks currently produced for Collatz by
the placeholder continuation blocks created after `break` and `return`.
Deleting uncalled functions is separate interprocedural work and is not part of
this first pass.

Implemented. On Collatz this removes six blocks created after `break` and
`return`, reducing the program from 26 blocks to 20 before later cleanup.

### 2. Thread Jumps And Remove Trivial Blocks

Resolve chains of empty unconditional-jump blocks to their final target, then
remove redundant trampoline blocks. Fold a branch into a jump when both targets
become identical. Preserve program and function entries even when they are
otherwise trivial.

Run this after unreachable-block deletion so dead predecessors do not prevent a
safe unique-predecessor or trampoline decision.

Implemented. It removes two trivial blocks from `toy_aes_round` and six more
from Collatz. Jump cycles are retained rather than threaded indefinitely.

### 3. Fold Branches With Identical Targets

After jump threading, replace `Branch` with `Jump` when both targets are the
same. Run trivial-block cleanup again because an empty branch block may become
a removable jump block.

Implemented. The reference programs do not contain this pattern after cleanup,
but a focused duplicated-target test removes the branch completely.

### 4. Fold Constant Expressions And Branches

Add a pure-expression simplifier for literal-only unary and binary operations,
casts, identities, and annihilators. Use the simplified condition to replace a
compile-time-known `Branch` with `Jump`, followed by another unreachable-block
cleanup.

Do not fold operations whose compile-time behavior would differ from runtime
wrapping, signed, division-by-zero, or fixed-point behavior.

### 5. Merge Identical Blocks

Redirect duplicate non-entry blocks to one canonical block when their complete
operation lists and terminators are equal. Keep program and function entries
distinct, then rerun identical-target branch folding and trivial-block cleanup.

Implemented. The reference programs contain no identical blocks after earlier
cleanup, but a focused repeated-branch test reduces two identical output
operations to one.

### 6. Remove Unused Functions

Starting only from the program entry, follow calls and retain functions reached
by an actual `Call`. Remove all blocks belonging only to unused functions and
compact IR function indexes while preserving their mapping to semantic function
frames.

Implemented. The reference programs contain no unused functions. Focused tests
cover removal of an earlier unused function while continuing to call a later
function with the correct parameter frame and return value.

### Selected CFG Cleanup Results

After items 1, 2, 3, 5, and 6:

| Program | Blocks | Dispatch groups | BF instructions | Dispatches | Executed ops |
| --- | ---: | ---: | ---: | ---: | ---: |
| `toy_aes_round` | 44 | 44 | 493,738 | 611 | 7,482,600 |
| `Collatz` | 14 | 13 | 51,408 | 103 | 107,125 |

Compared with the superblock baseline, BF size falls by 0.6% for AES and 19.5%
for Collatz. Dispatches fall by 2.9% and 24.3%, while deterministic executed
operations fall by 0.6% and 24.6%, respectively.

### 7. Expand BF Peephole Simplification

Replace the current single repeated-clear rewrite with a small token-aware pass
that cancels adjacent inverse operations:

- `><` and `<>`
- `+-` and `-+`
- repeated clears
- runs of identical pointer or cell operations when cancellation is possible

The pass must not move operations across loops or input/output instructions.
Measure both raw BF instructions and parsed `sanei` operations.

### 8. Track Known-Zero Control Cells

Teach BF code generation whether temporary and control cells are already known
to be zero. Avoid emitting a clear every time such a cell is allocated, and
invalidate the fact whenever generated code may leave a non-zero value.

Start with locally scoped control cells. Do not attempt whole-program tape-state
analysis in the first implementation.

### 9. Replace Large Compile-Time Unrolling With Runtime Loops

Identify fixed-count algorithms whose generated source grows linearly with the
iteration count, especially:

- the 16 shift steps used by fixed-point multiplication
- the 16 fractional digits considered by fixed-point decimal output

Replace an unrolled sequence only when a runtime BF loop reduces generated size
without an unacceptable execution-work increase. Keep constant small cases
unrolled when they remain cheaper.

### 10. Specialize Constant Arithmetic And Comparisons

Add compact code paths for common constant operands and zero tests before using
the generic multi-cell implementation. Prioritize operations visible in real
examples, such as adding small constants, multiplying by powers of two, and
comparing an `int` or `fixed` value with zero.

Avoid a large collection of speculative special cases; each specialization
must have a benchmark or code-size test demonstrating its value.

### 11. Compact Dynamic Wide-Array Access

Current dynamic `int` and `fixed` array access emits selection code proportional
to the declared array length. Design a stride-four runtime traversal that moves
or copies all four cells of the selected element without duplicating a case for
every index.

Cover loads, stores, raw input destinations where applicable, and formatted
input destinations. Preserve the current unchecked-index semantics unless a
separate language change explicitly adds bounds checking.

### 12. Evaluate Shared Runtime Microcode

Measure whether large repeated operations should be emitted once as internal
runtime routines rather than expanded at every source operation. Candidates
include:

- `int` and `fixed` multiplication and division
- signed and fixed decimal output
- formatted decimal input

This requires an internal calling or continuation mechanism and may increase
runtime dispatch work. Prototype one operation first and retain the design only
if programs with repeated uses become materially smaller.

### 13. Optimize Tape Layout From Measured Access Frequency

The BF generator pays one instruction per pointer step. Collect or estimate
which permanent cells are accessed most often and place hot runtime cells,
variables, spills, and temporaries closer together.

Do this only after earlier transformations stabilize the generated operation
mix. Compare total pointer-movement instructions as well as overall BF size and
executed operations.

### 14. Reconsider Global Control-Flow Transformations

After the preceding work, remeasure representative programs before considering:

- hierarchical or compressed PC dispatch
- trace-based block ordering
- selective function inlining
- tail-call lowering
- shared branch tails

These transformations have larger semantic and size tradeoffs. Do not introduce
SSA or a general interprocedural optimizer unless concrete programs demonstrate
that the simpler passes are insufficient.

### Backlog Completion Criteria

- each retained optimization has focused correctness tests
- benchmark history shows its individual effect
- generated BF remains valid on ordinary Brainfuck interpreters
- exact reference-program output remains unchanged
- documentation distinguishes implemented passes from deferred experiments

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

## Phase 5: Compile-Time Formatted Input

Keep `read` as the low-level input primitive. It must continue to consume
exactly one raw byte and may target only a byte variable or byte array element.
Do not make `read` behavior depend on the target type.

Add a separate formatted input expression whose surface mirrors formatted
output:

```sane
let left: int;
let right: int;
let matched: byte = input("{} {}", left, right);

if matched == 2 {
    print "sum: {}\n", left + right;
} else {
    puts "invalid input\n";
}
```

`input` is a compiler-recognized expression rather than an ordinary function.
Its first argument must be a compile-time string literal and its remaining
arguments must be writable scalar variables or array elements. No address-of
operator is introduced. Destination indexes are evaluated from left to right
when their corresponding placeholder is reached.

Initial placeholder support:

- `{}` and `{:d}` parse decimal text according to the destination type
  - `byte`: one or more decimal digits in the range 0 through 255
  - `int`: an optional leading `-` and one or more decimal digits in the signed
    32-bit range
- `{:c}` consumes one raw byte and requires a byte destination
- `{{` and `}}` match literal braces

Required matching and failure semantics:

- ordinary format characters must match input exactly
- format whitespace has no implicit C `scanf` behavior; it matches exactly
- each destination is updated only after its complete field parses
  successfully
- parsing stops at the first literal mismatch, invalid field, overflow, or EOF
- the expression returns the number of successfully assigned fields as a byte
- a failed field leaves its destination unchanged
- input already consumed while discovering a mismatch is not pushed back
- a trailing decimal placeholder ends at line feed, carriage return, or EOF

The implementation may retain one lookahead byte while matching a format. It
must not require a general input buffer, pointer type, or runtime format parser.
Formatting must be parsed and validated by the compiler, then lowered through
both the structured and PC pipelines.

The first version intentionally excludes:

- runtime format strings
- strings or variable-length byte sequences
- field widths and assignment suppression
- hexadecimal, binary, octal, floating-point, or unsigned-int formats
- automatic whitespace skipping
- locale-dependent behavior

Required tests:

- successful byte decimal, signed int decimal, and raw-byte fields
- multiple fields with literal separators and left-to-right assignment
- negative values and signed 32-bit boundaries
- invalid characters, empty fields, overflow, literal mismatch, and EOF
- destinations remain unchanged when their field fails
- conversion count stops at the first failure
- dynamic byte and int array-element destinations
- equivalent behavior in structured and PC backends
- existing raw-byte `read` tests and PC benchmark baselines remain unchanged

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

Function and PC debugging symbols are paused by request. The next work should
follow `docs/int-design.md`, beginning with scalar type and storage layout.
