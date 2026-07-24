# Language Reference

Sane 1.2 is a byte-oriented language. A `byte` is an unsigned 8-bit value
stored in one Brainfuck cell. Arithmetic wraps modulo 256. Zero is false and
every non-zero value is true.

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
`while`, `loop`, `for`, `break`, `continue`, `byte`, `put`, `puts`, `print`,
`println`, `read`, `true`, and `false`. They cannot be used as identifiers.

`const` became a reserved keyword in Sane 1.2; older source that used it as an
identifier must rename that identifier.

## Variables And Scope

Variables are lexically scoped. A typed declaration without an initializer
starts at zero.

```sane
let x: byte;
let y: byte = 65;
let z = 'A';
```

`let name = expr;` infers `byte`. Inner scopes may shadow outer names:

```sane
let x = 'A';

{
    let x = 'B';
    put x; // B
}

put x; // A
```

## Constants

Constants are compile-time byte values and do not allocate tape cells:

```sane
const BLOCK_SIZE = 16;
const NEWLINE = '\n';
const MASK = (1 << 4) - 1;

let state: byte[BLOCK_SIZE];
put NEWLINE;
```

Constants are lexically scoped and may use literals, earlier visible
constants, unary operators, and binary operators. Their arithmetic uses the
same wrapping byte semantics as runtime expressions. Constants may appear in
ordinary expressions, explicit array lengths, indexes, and array initializers.

Forward references, cyclic or self references, runtime variables, function
calls, and array accesses are not constant expressions. Explicit array lengths
must evaluate to a non-zero byte value. Inferred arrays may have length zero
when their byte element type is established by an empty string initializer.

## Arrays

Arrays are fixed-size byte arrays:

```sane
let data: byte[4];
let key: byte[4] = [1, 2, 3, 4];
let text: byte[6] = "hello\n";
```

When an initializer is present, the array length may be inferred:

```sane
let key = [1, 2, 3, 4];
let text = "hello\n";
```

List length is the number of initializer elements. String length is the number
of decoded ASCII bytes. An empty list is rejected because its element type
cannot be inferred; an empty string creates a zero-length byte array. Explicit
syntax remains required for an uninitialized array.

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

Constant indexes are checked at compile time. Dynamic indexes are not bounds
checked.

## Literals

Numeric literals may be decimal, binary, or hexadecimal:

```sane
65
0b01000001
0x41
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

## Assignment

Plain assignment replaces the destination byte:

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

`read` reads one byte. The bundled interpreter writes zero on EOF:

```sane
read x;
read data[i];
```

Output statements have distinct formatting behavior:

```sane
put 'A';          // one raw byte
puts "hello\n";  // string literal bytes
print 42;         // decimal text: 42
println 255;      // decimal text followed by newline
```

`print` also accepts a compile-time format string followed by byte
expressions:

```sane
print "round: {} value: {}\n", round, value;
print "player {:c} score: {:d}\n", player, score;
print "result: {{{}}}\n", result;
```

`{}` and `{:d}` write one argument as decimal text. `{:c}` writes one argument
as a raw byte, matching `put`. `{{` and `}}` write literal braces. The number of
placeholders must match the number of arguments. Output is streamed from left
to right, so each expression is evaluated when its placeholder is reached.
Runtime format strings and other format specifiers are not supported.

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

Functions accept byte parameters. A `-> byte` return type defines a byte
function:

```sane
fn add(a: byte, b: byte) -> byte {
    let result = a + b;
    return result;
}

println add(40, 2);
```

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

Calls may be standalone statements. A byte-returning call used as a statement
discards its result. A void call cannot be used where an expression value is
required.

Functions are currently supported only by the PC backend. Compile a function
program with `sanec`; the compiler automatically selects the PC backend when
functions are present. Use `sanec -b structured` only when intentionally
checking structured-backend compatibility.

Each function has a statically allocated frame for its parameters and local
variables. Nested calls are supported, but recursive call graphs are rejected.
The return-address stack allows at most 16 simultaneously active calls.

Every reachable path through a byte function must return a value with
`return expression;`. Void functions may use only `return;`. Both return forms
are valid only inside a function.

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
on all eight bits of a byte.

Division by zero is defined:

```sane
x / 0 == 0
x % 0 == x
```

## Current Limits

- The only value type is `byte`.
- Arrays contain bytes and have a compile-time length.
- Dynamic array indexes are not bounds checked.
- Function parameters and locals are bytes; functions return either a byte or
  no value.
- Recursive function call graphs are rejected.
- Source files are compiled independently; there are no modules.
