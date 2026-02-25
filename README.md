# Qwen BASIC Interpreter

A BASIC language interpreter written in Rust, implementing a subset of classic MS-BASIC (GW-BASIC style) with line-numbered programs, arithmetic expressions, string handling, and control flow.

The full language specification — EBNF grammar, semantic explanations, and example programs with expected output — is in [`qwen-basic-syntax.txt`](qwen-basic-syntax.txt).

## Language Features

### Statements

| Statement | Syntax | Description |
|-----------|--------|-------------|
| LET | `[LET] var = expr` | Assign a value to a variable (`LET` keyword is optional) |
| PRINT | `PRINT [expr_list]` | Output values to the screen |
| INPUT | `INPUT ["prompt";] var` | Read user input into a variable |
| IF/THEN | `IF expr THEN stmt\|linenum` | Conditional execution |
| GOTO | `GOTO linenum` | Unconditional jump to a line number |
| FOR/NEXT | `FOR var = start TO end [STEP val]` | Counted loop |
| REM | `REM text` or `' text` | Comment (ignored by interpreter) |
| END | `END` | Terminate the program |

### Expressions

- **Arithmetic**: `+`, `-`, `*`, `/`, `^` (power)
- **Comparison**: `=`, `<>`, `<`, `>`, `<=`, `>=`
- **Unary minus**: `-expr`
- **Parentheses**: `(expr)`
- **String concatenation**: `"A" + "B"`

Operator precedence (highest to lowest): parentheses, unary minus, `^`, `*` `/`, `+` `-`, comparisons.

### Built-in Functions

| Function | Description |
|----------|-------------|
| `INT(x)` | Floor of x |
| `ABS(x)` | Absolute value |
| `SQR(x)` | Square root |
| `RND(x)` | Random number in [0.0, 1.0) |
| `LEN(s$)` | Length of a string |

### Variables

Variable names are 1-2 alphanumeric characters, optionally followed by a type sigil:

- `$` — string (e.g., `N$`)
- `%` — integer (e.g., `X%`)
- `!` — single-precision float
- `#` — double-precision float

Keywords are case-insensitive.

### PRINT Formatting

- **Semicolon** (`;`) — suppresses the trailing newline, next output continues on the same line
- **Comma** (`,`) — advances to the next 14-character tab zone
- **No separator at end** — prints a newline after the last item

### Multi-Statement Lines

Multiple statements can appear on one line separated by colons:

```basic
50 ISPRIME = 0 : GOTO 70
```

## Project Structure

```
src/
├── main.rs          — CLI entry point: reads .bas files and runs them
├── token.rs         — Lexer/tokenizer: converts source text into tokens
├── expr.rs          — Expression parser: builds expression parse trees
├── eval.rs          — Evaluator: evaluates expressions, handles built-in functions
├── ast.rs           — Statement/program parser: parses tokens into an AST
└── interpreter.rs   — Interpreter: executes BASIC programs with full control flow
```

## Building

Requires Rust (edition 2021). Build with:

```sh
cargo build
```

For a release build:

```sh
cargo build --release
```

## Running

Pass a `.bas` file as an argument:

```sh
cargo run -- examples/hello.bas
```

Or use the compiled binary directly:

```sh
./target/release/qwen_basic myprogram.bas
```

### Example Program

Create a file `counter.bas`:

```basic
10 REM COUNTER PROGRAM
20 LET X = 1
30 PRINT "NUMBER:"; X
40 X = X + 1
50 IF X <= 3 THEN GOTO 30
60 PRINT "PROGRAM COMPLETE."
70 END
```

Run it:

```
$ cargo run -- counter.bas
NUMBER: 1
NUMBER: 2
NUMBER: 3
PROGRAM COMPLETE.
```

## Testing

Run the full test suite (157 tests):

```sh
cargo test
```

Tests are organized by module:

| Module | Tests | Coverage |
|--------|-------|----------|
| `token` | 11 | Lexer: numbers, strings, operators, keywords, sigils, remarks |
| `expr` | 22 | Expression parser: precedence, associativity, parentheses, functions |
| `eval` | 57 | Evaluator: arithmetic, comparisons, strings, all built-in functions, edge cases |
| `ast` | 26 | Statement parser: all statement types, multi-statement lines, full programs |
| `interpreter` | 41 | Integration: control flow, loops, I/O, example programs from the spec |

## Formatting

The project uses `rustfmt` with a 120-character line width (configured in `rustfmt.toml`):

```sh
cargo fmt
```
