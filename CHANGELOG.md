# Changelog

## 1.1.0

- Added the experimental `pc` backend with basic-block IR.
- Added byte functions for the `pc` backend: parameters, locals, returns, nested calls, and expression calls.
- Added static function frames, return-value spills, recursive-call rejection, and a 16-level return-pc stack limit.
- Added `examples/Collatz.sn` as a function-based example.
- Made `sanei` flush output after each Brainfuck `.` for interactive programs.
- Expanded tests for function semantics, pc backend behavior, and interpreter flushing.

## 1.0.0

- Initial stable Sane release.
- Added `sanec`, the Sane-to-Brainfuck compiler.
- Added `sanei`, an optimizing Brainfuck interpreter.
- Added byte variables, lexical scopes, fixed byte arrays, and array initializers.
- Added arithmetic, comparison, logical, bitwise, and shift operators.
- Added `if`, `while`, `loop`, `for`, `break`, and `continue`.
- Added `read`, `put`, `puts`, `print`, and `println`.
- Added source diagnostics with line/column caret output.
