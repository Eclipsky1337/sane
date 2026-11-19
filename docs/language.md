# Language Reference

Sane provides byte, signed integer, and fixed-point scalar values. A `byte` is
an unsigned 8-bit value stored in one Brainfuck cell. Arithmetic wraps modulo
256. Zero is false and every non-zero value is true for every scalar type.

## Source Files

Sane source files conventionally use the `.sn` extension. Statements end with
`;`, while blocks use braces.

```sane
let x = 65;
put x;
```

## Comments

Line comments start with `//`:

```sane
// This line is ignored.
let x = 65;
```

## Reserved Keywords

The reserved keywords are `let`, `const`, `fn`, `return`, `if`, `else`,
`while`, `loop`, `for`, `break`, `continue`, `byte`, `int`, `fixed`, `put`,
`puts`, `print`, `println`, `read`, `input`, `as`, `true`, and `false`. They
cannot be used as identifiers.

`const` became a reserved keyword in Sane 1.2; older source that used it as an
identifier must rename that identifier.

## Variables And Scope

Variables are lexically scoped. A typed declaration without an initializer
starts at zero.

```sane
let x: byte;
let y: byte = 65;
let z = 'A';
let wide: int = 300;
let ratio: fixed = 1.5;
```

`let name = expr;` infers `byte` for integer values through 255, `int` for
wider integer literals, and `fixed` for decimal-point literals. Inner scopes
may shadow outer names:

```sane
let x = 'A';

{
    let x = 'B';
    put x; // B
}

put x; // A
```

## Constants

Constants are compile-time byte, int, or fixed values and do not allocate tape
cells:

```sane
const BLOCK_SIZE = 16;
const NEWLINE = '\n';
const MASK = (1 << 4) - 1;
const WIDE_MASK = 0xffff0000;
const HALF = 0.5;

let state: byte[BLOCK_SIZE];
put NEWLINE;
```

Constants are lexically scoped and may use literals, earlier visible
constants, casts, unary operators, and binary operators. Their arithmetic uses
the same typed wrapping semantics as runtime expressions. Constants may appear
in ordinary expressions, explicit array lengths, indexes, and array
initializers.

Forward references, cyclic or self references, runtime variables, function
calls, and array accesses are not constant expressions. Explicit array lengths
must evaluate to a non-zero byte value. Inferred arrays may have length zero
when their byte element type is established by an empty string initializer.

## Arrays

Arrays are fixed-size and contain byte, int, or fixed elements:

```sane
let data: byte[4];
let key: byte[4] = [1, 2, 3, 4];
let text: byte[6] = "hello\n";
let totals: int[3] = [1, 65536, -2];
let points: fixed[3] = [1.25, -0.5, 3];
```

When an initializer is present, the array length may be inferred:

```sane
let key = [1, 2, 3, 4];
let text = "hello\n";
let totals = [1, 65536, -2];
let points = [1.25, -0.5, 3];
```

List length is the number of initializer elements. String length is the number
of decoded ASCII bytes. List initializers infer `int[]` when any element is an
int and `fixed[]` when any element is fixed; byte elements widen to the inferred
four-cell type. Mixing int and fixed elements requires explicit casts. An empty
list is rejected because its element type cannot be inferred; an empty string
creates a zero-length byte array. Explicit syntax remains required for an
uninitialized array. Int and fixed elements occupy four consecutive
little-endian cells.

Two-dimensional arrays use a page-oriented form:

```sane
let grid: int[4][256];
let cells: byte[100][10];

grid[row][col] = 42;
print "{}\n", grid[row][col];
```

The inner dimension may be 1 through 256. The total element count must stay
within the 4096-element cap, so the outer dimension can be larger when the
inner dimension is small. `a[i][j]` is flattened to `i * cols + j`; the page
walk selects the containing page, and the shared Tritonio body uses the low
byte as the in-page offset. Byte, int, and fixed elements are supported.

Two-dimensional arrays accept nested row initializers, a flat row-major list,
or a flat byte string:

```sane
let a: int[2][3] = [[1, 2, 3], [4, 5, 6]];
let b: byte[2][3] = ["abc", "def"];
let c: int[2][3] = [1, 2, 3, 4, 5, 6];
let d: byte[2][3] = "abcdef";
```

Each nested row must contain exactly `cols` elements. More than two dimensions
are rejected.

An initializer must contain exactly the declared number of bytes. String
initializers are ASCII-only and do not receive an implicit trailing `\0`.

Array elements support expressions, assignment, compound assignment, input,
and output:

```sane
data[0] = 65;
data[i] ^= key[i];
read data[i];
put data[i];
```

Constant indexes are checked at compile time. Dynamic indexes may be byte or
int and are not bounds checked. Array lengths are capped at 4096 elements.
Arrays longer than 256 elements are internally split into 256-element pages;
use an int index to reach pages after the first, because a byte index only
addresses page 0.

## Literals

Numeric literals may be decimal, binary, or hexadecimal:

```sane
65
0b01000001
0x41
300
0xffffffff
1.0
12.5
0.125
```

Character and string literals are ASCII-only:

```sane
'A'
'\n'
"hello\n"
```

Supported escapes are `\n`, `\r`, `\t`, `\0`, `\\`, `\'`, and `\"`.

Boolean literals are byte values:

```sane
true  // 1
false // 0
```

`int` is a signed 32-bit two's-complement value stored in four little-endian
cells. Current int arithmetic wraps modulo 2^32. Addition, subtraction,
negation, multiplication, division, remainder, signed comparisons, logical and
bitwise operations, shifts, and use as a condition are implemented. Division
truncates toward zero, remainder keeps the dividend's sign, and right shift is
arithmetic and preserves the sign bit. Division by zero returns `0`; remainder
by zero returns the dividend.

Bytes widen to ints implicitly. Narrowing requires an explicit cast:

```sane
let small: byte = 65;
let wide: int = small;
let low: byte = wide as byte;
```

`fixed` is a signed Q16.16 value stored in four little-endian cells. Its raw
signed 32-bit representation is divided by 65,536, giving a range from
`-32768.0` through `32767.9999847412109375` and a resolution of
`0.0000152587890625`. Fixed arithmetic wraps modulo 2^32.

A decimal point creates a fixed literal. Conversion is performed with exact
integer arithmetic and truncates toward zero when the decimal fraction is not
exactly representable. Scientific notation and literals without a digit before
the decimal point are not supported.

Fixed values support negation, logical negation, addition, subtraction,
multiplication, division, comparisons, logical operations, conditions, and the
matching arithmetic compound assignments. Division truncates toward zero and
returns `0.0` when its divisor is zero. Remainder, bitwise operations, and
shifts are not defined for fixed values.

Bytes widen to fixed implicitly. Int/fixed conversions and fixed narrowing are
explicit:

```sane
let count: int = 12;
let ratio: fixed = count as fixed;
let whole: int = ratio as int;
let low: byte = ratio as byte;
```

`int as fixed` shifts left by 16 bits with wrapping. `fixed as int` truncates
toward zero. `fixed as byte` performs the same truncation and keeps the low
eight bits. Int and fixed values do not mix implicitly.

## Assignment

Plain assignment replaces the destination value:

```sane
x = expression;
data[i] = expression;
```

Compound assignments are available for arithmetic, bitwise operations, and
shifts:

```sane
x += expression;
x -= expression;
x *= expression;
x /= expression;
x %= expression;
x &= expression;
x |= expression;
x ^= expression;
x <<= expression;
x >>= expression;
```

The same operators work on array elements.

## Input And Output

`read` reads one raw byte. The bundled interpreter writes zero on EOF. Its
target must be a byte variable or byte array element:

```sane
read byte_value;
read data[i];
```

Wider values are parsed explicitly by repeatedly reading bytes and combining
their digits.

`input` provides compile-time checked formatted input and returns the number of
successfully assigned fields as a byte:

```sane
let left: int;
let right: int;
let matched: byte = input("{} {}", left, right);
```

The first argument must be a string literal. Remaining arguments must be
writable scalar variables or array elements. Supported placeholders are:

- `{}` and `{:d}` parse one or more decimal digits into a byte, or an optional
  leading `-` followed by digits into an int.
- `{:c}` consumes one raw byte and requires a byte destination.
- `{{` and `}}` match literal braces.

Ordinary characters, including whitespace, match exactly. Input stops at the
first mismatch, invalid field, overflow, or EOF. A destination is updated only
after its complete field succeeds, while earlier successful destinations stay
updated. Consumed mismatching bytes are not returned to the input stream. A
final decimal field must end at line feed, carriage return, or EOF; the line
ending is consumed. Decimal fixed input is not supported in this first version.

Array destination indexes are evaluated from left to right when their fields
are reached:

```sane
let index: byte;
let values: int[4];
let matched = input("{} {}", index, values[index]);
```

`input` is a compiler-recognized expression rather than an ordinary function.

Output statements have distinct formatting behavior:

```sane
put 'A';          // one raw byte
puts "hello\n";  // string literal bytes
print 42;         // decimal text: 42
println 255;      // decimal text followed by newline
```

`print` also accepts a compile-time format string followed by scalar
expressions:

```sane
print "round: {} value: {}\n", round, value;
print "player {:c} score: {:d}\n", player, score;
print "result: {{{}}}\n", result;
```

`{}` and `{:d}` write a byte, signed int, or fixed argument as decimal text.
Fixed output is exact, includes at least one fractional digit, and removes
unnecessary trailing zeroes. `{:c}` requires a byte and writes it raw, matching
`put`. `{{` and `}}` write literal braces. The number of placeholders must
match the number of arguments. Output is streamed from left to right, so each
expression is evaluated when its placeholder is reached. Runtime format
strings and other format specifiers are not supported.

## Control Flow

Conditions use byte truthiness: zero is false and non-zero is true.

### Conditions

```sane
if x == 0 {
    puts "zero\n";
} else if x < 10 {
    puts "small\n";
} else {
    puts "large\n";
}
```

### While Loops

```sane
while x < 10 {
    x += 1;
}
```

### Infinite Loops

```sane
loop {
    if done {
        break;
    }
}
```

### For Loops

```sane
for let i = 0; i < 4; i += 1 {
    put data[i];
}
```

The initializer and condition may be omitted:

```sane
for ;; {
    break;
}
```

`break` exits the nearest loop. `continue` skips the remaining body. In a
`for` loop, `continue` still runs the step expression before rechecking the
condition.

## Functions

Function parameters and return values may be `byte`, `int`, or `fixed`:

```sane
fn add(a: byte, b: byte) -> byte {
    let result = a + b;
    return result;
}

println add(40, 2);

fn add_wide(a: int, b: int) -> int {
    return a + b;
}

fn midpoint(a: fixed, b: fixed) -> fixed {
    return (a + b) / 2.0;
}
```

Byte arguments and return expressions widen to `int` or `fixed` implicitly.
Int and fixed values require explicit casts when crossing those type
boundaries or when a byte result is required.

Function calls are expressions and may be nested or combined with other
operators:

```sane
let x = add(1, 2) + add(3, 4);
return add(x, add(5, 6));
```

Omitting the return type defines a void function. Void functions may use
`return;` or fall through the end of their body:

```sane
fn show(value: byte) {
    println value;
}

show(42);
```

Calls may be standalone statements. A value-returning call used as a statement
discards its result. A void call cannot be used where an expression value is
required.

All programs, including those with functions, compile through the PC backend.

Each function has a statically allocated frame for its parameters and local
variables. Nested calls are supported, but recursive call graphs are rejected.
The return-address stack allows at most 16 simultaneously active calls.

Every reachable path through a byte, int, or fixed function must return a value
with `return expression;`. Void functions may use only `return;`. Both return
forms are valid only inside a function.

## Expressions

Expression forms include:

```text
number
true
false
character
variable
array[index]
function(arguments)
(expression)
!expression
~expression
expression operator expression
```

Operator precedence, from tightest to loosest:

| Precedence | Operators |
| --- | --- |
| 1 | Parentheses |
| 2 | `!`, `~` |
| 3 | `*`, `/`, `%` |
| 4 | `+`, `-` |
| 5 | `<<`, `>>` |
| 6 | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| 7 | `&` |
| 8 | `^` |
| 9 | `|` |
| 10 | `&&` |
| 11 | `||` |

Logical and comparison operations produce `0` or `1`. Bitwise operations work
on all eight bits of a byte or all 32 bits of an int and are unavailable for
fixed values.

Division by zero is defined:

```sane
x / 0 == 0
x % 0 == x
```

## Current Limits

- Arrays have a compile-time length and cannot mix element types implicitly.
- Dynamic array indexes are not bounds checked.
- Recursive function call graphs are rejected.
- Source files are compiled independently; there are no modules.
