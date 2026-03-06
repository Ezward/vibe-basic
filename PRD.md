We are going to create a basic interpreter written in Rust for a subset of MS-BASIC.
- The BASIC language syntax is specified as an eBNF grammar in this file; vibe-basic-syntax.txt.
  - The file also explains the semantics of the language and provides several program examples with outputs.

1. First, create a parser for expressions that create and expression parse tree.
  - include extensive unit tests that test the the structure of the expression parse tree.

2. Next, create a stack-based looping evaluator (rather than a recursive evaluator) that can evaluate an expression parse tree and return a value.
  - include unit tests that check various kinds of expressions and their evaluation.

3. Next, create a parser for basic statements and programs that parses them into an abstract syntax tree.
  - include unit tests that check the structure of the abstract syntax tree.
  - The statement parser should skip whitespace at the beginning of a line
  - The statement parser should ignore empty lines.
  - Parse errors should show the 1-based file line number and full source text for the line that caused the error.

4. Next, create an interpreter for basic programs that will run the program and write output to stdout.
  - Runtime errors should show the 1-based file line number and full source text for the line that caused the error.
  - include unit tests that run example programs and checks their output.

5. Next, add a debug mode to the interpreter.
  - The command line should support opening the program in debug mode by providing the --debug argument.
  - The debug mode should open the file, but not immediately execute it; instead it should accept debug commands and execute them.  The debug commands are:
    - RUN: this debugger command runs the program from the current line, initially ignoring any breakpoint, until it either finishes or errors or hits a breakpoint.
      - in debug mode, if the BASIC program finishes without error, then the debugger should continue to run, maintaining the BASIC program state and allowing the user to enter debugger commands.
      - in debug mode, errors should cause the debugger to break at the line that caused the error rather than to exit the program and quit the interpreter.
    - BREAK: this debugger commands sets and remembers a breakpoint.  The program should remember all breakpoints and test them before executing each line of of the BASIC program.  The BREAK command takes one argument.  The argument can be:
      - "AT" followed by a basic line number, like "300".  This would cause the debugger to stop if the interpreter is about to execute line 300 (so line 300 is the current line, but we break before it is executed)
      - "IF" followed by a logical expression that yields a true or false, as would be encountered in an normal BASIC IF statement.  If the expression evaluates to true, then the program breaks at the current line (the line that is about to execute).  If the expression evaluates to false, then then program continues to run.
    - STEP: This debugger command single steps the program (executes the current line; ignoring any breakpoint) then breaks before it executes the next line.
    - GOTO: This debugger command takes one argument, a BASIC line number, and sets the current line to the given line number, then continues to execute
    - LET: This debugger command works just like the LET statement in BASIC; it allows for setting the value of a variable.
    - PRINT: This debugger command allows the user to print just like the normal BASIC PRINT statement.
    - LIST: print the BASIC program.
      - Optionally it can take a BASIC line number that represents the first line to print.  So LIST 300 would start the printing at BASIC line 300.
      - Optionally, after the first line number it can take a second line number that represents the last line to print.  So LIST 300 400 would print all basic lines from 300 to an including line 400.
    - QUIT: This debugger command exits the program and quits back to the operating system.
    - HELP: Print the list of debugging commands and how they are used.




Generally:

- Use the Rust language to implement the code.
- Always run `cargo fmt` on the code to ensure standard formatting.  Use a 120 character line length.
- Always include unit tests for happy paths and edge cases.  Unit tests should use randomized data where possible to avoid tests that only pass with a fixed data set.
- Always make sure the program will compile without errors or warnings.  Fix any errors or warnings.
- Always make sure the unit tests compile without errors or warnings and the tests run without failures.   Fix any errors or warnings or test failures.
- Always document functions.
