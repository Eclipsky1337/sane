# Changelog

## Unreleased

- Added `{:c}` raw-byte and `{:d}` explicit-decimal placeholders to formatted
  `print` output while keeping `{}` as the decimal shorthand.

## 1.2.0

- Made backend selection automatic: function-free programs use the structured
  backend, while programs containing functions use the PC backend.
- Added void functions, `return;`, standalone call statements, and discarded
  results from byte-returning calls.
- Added inferred byte-array declarations from list and string initializers.
- Added lexically scoped compile-time byte constants and constant array
  lengths. `const` is now a reserved keyword.
- Added compile-time checked formatted `print` output with decimal `{}`
  placeholders and escaped braces.
- Updated the AES and Collatz examples for the new syntax and removed the
  obsolete Luhn example.

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
