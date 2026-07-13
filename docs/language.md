# Language Reference

Sane 1.0 is a byte-oriented language. A `byte` is an unsigned 8-bit value
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

## Arrays

Arrays are fixed-size byte arrays:

```sane
let data: byte[4];
let key: byte[4] = [1, 2, 3, 4];
let text: byte[6] = "hello\n";
```

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
checked in Sane 1.0.

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

Functions accept byte parameters and return one byte:

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

Functions are currently supported only by the PC backend. Compile a function
program with `sanec -b pc`.

Each function has a statically allocated frame for its parameters and local
variables. Nested calls are supported, but recursive call graphs are rejected.
The return-address stack allows at most 16 simultaneously active calls.

Every reachable path through a function must return a value. `return` is valid
only inside a function.

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
- Function parameters, locals, and return values are bytes.
- Recursive function call graphs are rejected.
- Source files are compiled independently; there are no modules.
