# 32-Bit Integer Design

This document defines the implemented `int` type and records its staged
implementation. Function and PC debug-symbol work remains separately deferred.

## Representation

- `int` is a signed 32-bit two's-complement value.
- Its range is `-2147483648` through `2147483647`.
- Each value occupies four consecutive Brainfuck cells.
- Cells use little-endian order: the base cell stores bits 0 through 7 and
  `base + 3` stores bits 24 through 31.
- Arithmetic overflow wraps modulo 2^32.
- A condition is false only when all four cells are zero.

`byte` remains an unsigned wrapping 8-bit type occupying one cell.

## Types And Literals

The compiler will introduce a scalar type model shared by semantic resolution,
IR lowering, and Brainfuck generation.

- Unsuffixed non-negative literals from 0 through 255 infer `byte` when there
  is no wider context.
- Larger decimal literals infer `int` and must fit the signed 32-bit range.
- Negative literals require unary `-` and infer `int`.
- Hexadecimal and binary `int` literals may describe any 32-bit bit pattern.
- Mixed `byte` and `int` binary expressions promote the `byte` operand to
  `int`.

## Conversions

- `byte` to `int` is an implicit zero-extension.
- `int` to `byte` requires an explicit `as byte` conversion and retains the
  low eight bits.
- `value as int` explicitly zero-extends a `byte` and leaves an `int`
  unchanged.
- Assignments and function arguments never narrow implicitly.

## Tape Layout

Scalar storage is represented by a layout containing a base cell and value
type. Allocators reserve the complete width and scoped reuse must only reuse a
region with a compatible width.

PC runtime values use these widths:

- byte return value: one `rv` cell
- int return value: four consecutive `rv` cells
- byte spill: one cell
- int spill: four consecutive cells

`int[]` elements use four-cell strides. Every `int[]` reserves a 16-cell prefix
for four interleaved stride-four dynamic-access lanes, regardless of length.
Byte arrays retain their existing single-cell element layout.

## Operations

Implementation order:

1. scalar type and storage layout
2. literals, declarations, assignment, and casts
3. zero tests, addition, subtraction, equality, and inequality
4. signed ordering, bitwise operations, and shifts
5. multiplication, division, and remainder
6. signed decimal output and compile-time formatted decimal input
7. function parameters, returns, and typed spills
8. four-cell array elements and dynamic indexing

Each stage must preserve existing `byte` behavior in the PC backend.
Operations not implemented for `int` must produce a semantic
diagnostic rather than silently compiling as byte operations.

## Deferred Decisions

- Source-level checked arithmetic is not part of the initial implementation.
- Wider types, unsigned 32-bit source types, pointers, and mixed-width arrays
  remain out of scope.
- Debug-symbol syntax for multi-cell values will be designed when typed values
  reach code generation.
