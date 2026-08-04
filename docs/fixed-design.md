# Q16.16 Fixed-Point Design

This document defines Sane's initial `fixed` type. The implementation extends
the existing four-cell scalar, function, array, constant, and backend machinery
without changing `byte`, `int`, or raw-byte `read` behavior.

## Representation

- `fixed` is a signed Q16.16 two's-complement value.
- Each value occupies four consecutive little-endian Brainfuck cells.
- The stored signed 32-bit integer is the real value multiplied by 65,536.
- The range is `-32768.0` through `32767.9999847412109375`.
- The resolution is `1 / 65536`, or `0.0000152587890625`.
- Arithmetic overflow wraps modulo 2^32, matching `int`.
- A condition is false only when all four cells are zero.

## Literals

- A decimal point produces a `fixed` literal: `1.0`, `12.5`, `0.125`.
- At least one digit is required before the decimal point in the first version.
- Scientific notation is not supported initially.
- Literal conversion uses exact decimal integer arithmetic, not host floating
  point.
- Values that are not exactly representable are truncated toward zero to the
  nearest Q16.16 value.
- Positive literals must fit the positive fixed range.
- `-32768.0` is accepted through unary negation of the boundary magnitude.

## Conversions

- `byte` to `fixed` is an implicit zero-extension followed by a 16-bit scale.
- `int` to `fixed` requires `as fixed` and shifts left by 16 bits with wrapping
  overflow.
- `fixed` to `int` requires `as int` and truncates toward zero.
- `fixed` to `byte` requires `as byte`, truncates toward zero, and retains the
  low eight bits of the integer result.
- `fixed as fixed` is unchanged.
- Assignments, array elements, function arguments, and returns never narrow or
  convert between `int` and `fixed` implicitly.

## Operations

Supported operations:

- unary `-` and `!`
- `+`, `-`, `*`, `/`
- `==`, `!=`, `<`, `<=`, `>`, `>=`
- `&&`, `||`
- conditions in control flow
- corresponding arithmetic compound assignments

Addition, subtraction, negation, equality, ordering, and zero tests operate on
the stored signed 32-bit representation.

Multiplication computes the equivalent of a signed 32-by-32-bit product in a
64-bit intermediate, divides the magnitude by 65,536, truncates toward zero,
restores the sign, and retains the low 32 result bits. The backend may use
16-bit partial products as long as it preserves those exact wide semantics.

Division computes `(left * 65536) / right` with a signed wide numerator,
truncates toward zero, and retains the low 32 result bits. Division by zero
returns zero, matching `int` division.

The first version does not define `%`, bitwise operators, or shifts for
`fixed`; source code must cast to `int` when raw-bit operations are intended.

## Constants, Functions, And Arrays

- Compile-time constants preserve the `fixed` type and evaluate with runtime
  Q16.16 semantics.
- Function parameters, returns, PC return values, and spills use four cells.
- `fixed[]` elements use four-cell strides and support constant and dynamic
  byte indexes.
- Mixed-width arrays remain unsupported.

## Input And Output

- `read` remains a raw one-byte operation and cannot target `fixed`.
- The first formatted `input` version supports byte and int decimal fields;
  fixed decimal parsing remains explicit source code for now.
- `print`, `println`, `{}`, and `{:d}` render signed fixed-point decimal text.
- Output is exact for the stored Q16.16 value, emits at least one fractional
  digit, and removes unnecessary trailing fractional zeroes.
- `{:c}` and `put` require byte values; fixed values need an explicit cast.

## Implementation Order

1. type syntax, decimal literals, layouts, and semantic conversions
2. storage, copies, casts, conditions, addition, subtraction, and comparisons
3. signed 64-bit intermediate primitives
4. multiplication and division
5. constants, functions, typed spills, and arrays
6. exact decimal output
7. structured and PC regression tests, examples, and documentation

Every stage must preserve existing byte and int outputs and the measured PC
benchmark baselines.
