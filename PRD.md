We are going to create a basic interpreter written in Rust for a subset of MS-BASIC.
- The BASIC language syntax is specified as an eBNF grammar in this file; vibe-basic-syntax.txt.
  - The file also explains the semantics of the language and provides several program examples with outputs.

1. First, create a parser for expressions that create and expression parse tree.
  - include extensive unit tests that test the the structure of the expression parse tree.

2. Next, create a stack-based looping evaluator (rather than a recursive evaluator) that can evaluate an expression parse tree and return a value.
  - include unit tests that check various kinds of expressions and their evaluation.

3. Make sure all the built-in functions specified in section 10 (10. Built-in Functions) of ./vibe-basic-syntax.txt are implemented.  Add unit tests for all functions.
  - create an example program called ./examples/strings.bas that tests all the string functions.
  - create an example program called ./examples/math.bas that tests all the numeric functions.

4. Next, create a parser for basic statements and programs that parses them into an abstract syntax tree.
  - include unit tests that check the structure of the abstract syntax tree.
  - The statement parser should skip whitespace at the beginning of a line
  - The statement parser should ignore empty lines.
  - Parse errors should show the 1-based file line number and full source text for the line that caused the error.

5. Next, create an interpreter for basic programs that will run the program and write output to stdout.
  - Runtime errors should show the 1-based file line number and full source text for the line that caused the error.
  - include unit tests that run example programs and checks their output.

6. Make sure the DEF FN user definable function syntax is implemented as specified in section 8 (8. DEF FN (User-Defined Functions)) of vibe-basic-syntax.txt.  Add unit tests.

7. Next, add a debug mode to the interpreter.
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

8. GW-BASIC supports a DATA statement and a READ statement to read the data and a RESTORE statement that allows DATA statements to be reread from a specified line.   See https://hwiegman.home.xs4all.nl/gw-man/DATA.html and https://hwiegman.home.xs4all.nl/gw-man/READ.html and https://hwiegman.home.xs4all.nl/gw-man/RESTORE.html respectively.  Implement these statements and related unit tests.  Implement an example program that tests these statements and save as ./examples/data.bas.  Make sure this parses and runs correctly.

9. GW-BASIC supports multidimensional arrays using the DIM statement.  The ERASE statement can be used to eliminate arrays from a program to save memory or to allow them to be redimensioned.  See '6.2.3 Array Variables' section of https://hwiegman.home.xs4all.nl/gw-man/ for an explanation of array variables.  See https://hwiegman.home.xs4all.nl/gw-man/DIM.html and https://hwiegman.home.xs4all.nl/gw-man/ERASE.html respectively for explanation of the related statements.  Implement the array syntax and these related statements.  Implement an example program that tests these statements and save as ./examples/arrays.bas. Make sure this parses and runs correctly.

10. Most BASIC interpreters include the GOSUB...RETURN and ON...GOSUB..RETURN statements that branch-to and return-from a subroutine.  GW-BASIC allows the RETURN statement to take an optional expression argument that should evaluate to a BASIC line number.  Implement the GOSUB...RETURN and ON...GOSUB...RETURN statements in the parser and the interpreter.
- The GOSUB clause in the GOSUB...RETURN statement takes a single expression argument that should evaluate to a BASIC line number, which is used as the first line number of the subroutine.  See https://hwiegman.home.xs4all.nl/gw-man/GOSUB.html.
- The GOSUB clause in the ON...GOSUB..RETURN statement takes a list of BASIC line numbers as an argument; the ON...GOSUB...RETURN statement looks like this, `ON offset GOSUB address0, address1, ...addressN` where offset is an expression that evaluates to an index that is used to index into the list of addresses provided as arguments to the GOSUB clause.  By using an expression to calculate where the GOSUB jumps to, this statement can replace many IF...THEN..ELSE chains.  See https://picaxe.com/basic-commands/program-flow-control/on-gosub/.
- The RETURN statement in a subroutine causes GW-BASIC to return to the statement following the most recent GOSUB clause (which may be on the same line as part of a multi-statment line).  For this reason, when executing a GOSUB statement, the intepreter should remember (on a stack) the statement after the GOSUB clause so that the RETURN statement can pop that value.  If the optional argument is provided, then the RETURN statement will still pop the subroutine stack, but instead of using the popped value to return it will use the provided BASIC line number argument.  See https://hwiegman.home.xs4all.nl/gw-man/RETURN.html.
Write an example program that tests GOSUB...RETURN and ON...GOSUB...RETURNand save as ./examples/gosub.bas.  Make sure this parses and runs correctly.

11. Make sure that mixing IF...THEN...ELSE, FOR...NEXT, GOSUB...RETURN, ON...GOSUB...RETURN, GOTO and END nested in combinations in a multiline statement will work correctly.  Specifically the interpreter should track both the BASIC line number and the statement index to which it should jump, loop or return.

12. Please create a text adventure game based on the key elements and plot points in the book 'Alice in Wonderland' whose text can be found at https://www.gutenberg.org/files/11/11-0.txt and save the text adventure as ./examples/alice.bas. Add interesting puzzles to the game.  Make sure the BASIC code parses correctly. Make sure the adventure can be solved. Make sure all necesary puzzles are solved before the player can win.

Generally:

- Use the Rust language to implement the code.
- Always make sure the program will compile without errors or warnings.  Fix any errors or warnings.
- Always include unit tests for happy paths and edge cases.  Unit tests should use randomized data where possible to avoid tests that only pass with a fixed data set.
- Use `cargo-llvm-cov` to determine test coverage.  Add missing unit tests for happy path and edge cases to achieve 100% test coverage.
- Always make sure the unit tests compile without errors or warnings and the tests run without failures.   Fix any errors or warnings or test failures.
- Always make sure to lint the code with `cargo clippy` and fix any issues.
- Always run `cargo fmt` on the code to ensure standard formatting.  Use a 120 character line length.

- Always document functions.
