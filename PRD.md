We are going to create a basic interpreter written in Rust for a subset of MS-BASIC.
- The BASIC language syntax is specified as an eBNF grammar in this file; qwen-basic-syntax.txt.
  - The file also explains the semantics of the language and provides several program examples with outputs.

1. First, create a parser for expressions that create and expression parse tree.
  - include extensive unit tests that test the the structure of the expression parse tree.

2. Next, create a stack-based evaluator that can evaluate an expression parse tree and return a value.
  - include unit tests that check various kinds of expressions and their evaluation.

3. Next, create a parser for basic statements and programs that parses them into an abstract syntax tree.
  - include unit tests that check the structure of the abstract syntax tree.
  - The statement parser should skip whitespace at the beginning of a line
  - The statement parser should ignore empty lines.
  - Parse errors should show the 1-based file line number and full source text for the line that caused the error.

4. Next, create an interpreter for basic programs that will run the program and write output to stdout.
  - Runtime errors should show the 1-based file line number and full source text for the line that caused the error.
  - include unit tests that run example programs and checks their output.


Generally:

- Use the Rust language to implement the code.
- Always run `cargo fmt` on the code to ensure standard formatting.  Use a 120 character line length.
- Always include unit tests for happy paths and edge cases.
- Always make sure the program will compile without errors or warnings.  Fix any errors or warnings.
- Always make sure the unit tests compile without errors or warnings and the tests run without failures.   Fix any errors or warnings or test failures.
