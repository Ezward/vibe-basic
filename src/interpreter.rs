//! Program interpreter (runtime execution engine) for BASIC.
//!
//! This module provides the `Interpreter` struct which executes a parsed BASIC
//! `Program`. It manages control flow (sequential execution, GOTO, IF/THEN branching,
//! FOR/NEXT loops), performs I/O via generic `BufRead`/`Write` streams, and delegates
//! expression evaluation to the `Evaluator`. PRINT formatting follows MS-BASIC
//! conventions (14-character tab zones for commas, newline suppression with semicolons).

use crate::ast::{PrintItem, Program, Statement, ThenClause};
use crate::eval::{Evaluator, Value};
use std::io::{BufRead, Write};

/// Tracks the state of an active FOR loop on the loop stack.
#[derive(Debug, Clone)]
struct ForState {
    variable: String,
    end_val: f64,
    step_val: f64,
    /// Index into program lines where the FOR statement is
    line_index: usize,
}

/// Runtime interpreter for BASIC programs, parameterized over input and output streams.
pub struct Interpreter<R: BufRead, W: Write> {
    pub(crate) evaluator: Evaluator,
    pub(crate) input: R,
    pub(crate) output: W,
    for_stack: Vec<ForState>,
    column: usize,
}

impl<R: BufRead, W: Write> Interpreter<R, W> {
    /// Creates a new interpreter with the given input and output streams.
    pub fn new(input: R, output: W) -> Self {
        Interpreter {
            evaluator: Evaluator::new(),
            input,
            output,
            for_stack: Vec::new(),
            column: 0,
        }
    }

    /// Executes a BASIC program from the first line to completion. Handles sequential
    /// execution, GOTO jumps, IF/THEN branching, FOR/NEXT loops, and END termination.
    /// Errors include source line context for debugging.
    pub fn run(&mut self, program: &Program) -> Result<(), String> {
        if program.lines.is_empty() {
            return Ok(());
        }

        let mut line_idx = 0;

        while line_idx < program.lines.len() {
            let line = &program.lines[line_idx];
            let mut stmt_idx = 0;
            let mut next_line_idx = line_idx + 1;

            while stmt_idx < line.statements.len() {
                let stmt = &line.statements[stmt_idx];
                let result = self.execute_statement(stmt, line_idx, program).map_err(|e| {
                    let source_text = program
                        .source_lines
                        .get(line.source_line - 1)
                        .map(|s| s.as_str())
                        .unwrap_or("<unknown>");
                    format!("{}\n  at line {}: {}", e, line.source_line, source_text)
                });
                match result? {
                    StmtResult::Continue => {
                        stmt_idx += 1;
                    }
                    StmtResult::Goto(target_line) => {
                        next_line_idx = self.find_line_index(program, target_line).map_err(|e| {
                            let source_text = program
                                .source_lines
                                .get(line.source_line - 1)
                                .map(|s| s.as_str())
                                .unwrap_or("<unknown>");
                            format!("{}\n  at line {}: {}", e, line.source_line, source_text)
                        })?;
                        break;
                    }
                    StmtResult::End => return Ok(()),
                    StmtResult::SkipLine => {
                        // IF condition was false - skip remaining statements on this line
                        break;
                    }
                    StmtResult::ForLoopSkip(target_idx) => {
                        next_line_idx = target_idx;
                        break;
                    }
                }
            }

            line_idx = next_line_idx;
        }
        Ok(())
    }

    /// Finds the index of a BASIC line by its line number (for GOTO targets).
    pub(crate) fn find_line_index(&self, program: &Program, target_line: u32) -> Result<usize, String> {
        program
            .lines
            .iter()
            .position(|l| l.line_number == target_line)
            .ok_or_else(|| format!("Line {} not found", target_line))
    }

    /// Executes a single statement and returns a `StmtResult` indicating control flow:
    /// continue to next statement, jump to a line, end the program, skip remaining
    /// statements on the current line, or skip past a FOR loop body.
    pub(crate) fn execute_statement(
        &mut self,
        stmt: &Statement,
        current_line_idx: usize,
        program: &Program,
    ) -> Result<StmtResult, String> {
        match stmt {
            Statement::Let { variable, expression } => {
                let value = self.evaluator.eval_expr(expression)?;
                self.evaluator.variables.insert(variable.clone(), value);
                Ok(StmtResult::Continue)
            }
            Statement::Print { items } => {
                self.execute_print(items)?;
                Ok(StmtResult::Continue)
            }
            Statement::If { condition, then, else_clause } => {
                let val = self.evaluator.eval_expr(condition)?;
                if val.is_truthy() {
                    match then.as_ref() {
                        ThenClause::LineNumber(n) => Ok(StmtResult::Goto(*n)),
                        ThenClause::Statement(inner_stmt) => {
                            self.execute_statement(inner_stmt, current_line_idx, program)
                        }
                    }
                } else if let Some(else_cl) = else_clause {
                    match else_cl.as_ref() {
                        ThenClause::LineNumber(n) => Ok(StmtResult::Goto(*n)),
                        ThenClause::Statement(inner_stmt) => {
                            self.execute_statement(inner_stmt, current_line_idx, program)
                        }
                    }
                } else {
                    Ok(StmtResult::SkipLine)
                }
            }
            Statement::Goto { line_number } => Ok(StmtResult::Goto(*line_number)),
            Statement::Input { prompt, variable } => {
                self.execute_input(prompt.as_deref(), variable)?;
                Ok(StmtResult::Continue)
            }
            Statement::For {
                variable,
                start,
                end,
                step,
            } => {
                let start_val = self.evaluator.eval_expr(start)?.as_number()?;
                let end_val = self.evaluator.eval_expr(end)?.as_number()?;
                let step_val = match step {
                    Some(s) => self.evaluator.eval_expr(s)?.as_number()?,
                    None => 1.0,
                };

                self.evaluator
                    .variables
                    .insert(variable.clone(), Value::Number(start_val));

                // Check if the loop should be skipped entirely
                if (step_val > 0.0 && start_val > end_val) || (step_val < 0.0 && start_val < end_val) {
                    // Skip to after the matching NEXT
                    let next_idx = self.find_matching_next(program, current_line_idx, variable)?;
                    return Ok(StmtResult::ForLoopSkip(next_idx + 1));
                }

                self.for_stack.push(ForState {
                    variable: variable.clone(),
                    end_val,
                    step_val,
                    line_index: current_line_idx,
                });

                Ok(StmtResult::Continue)
            }
            Statement::Next { variable } => {
                let for_state = if let Some(var_name) = variable {
                    // Find the matching FOR on the stack
                    let idx = self
                        .for_stack
                        .iter()
                        .rposition(|f| f.variable == *var_name)
                        .ok_or_else(|| format!("NEXT without FOR for variable {}", var_name))?;
                    self.for_stack[idx].clone()
                } else {
                    self.for_stack.last().cloned().ok_or("NEXT without FOR")?
                };

                // Increment the counter
                let current = self
                    .evaluator
                    .variables
                    .get(&for_state.variable)
                    .ok_or_else(|| format!("Variable {} not found", for_state.variable))?
                    .as_number()?;

                let new_val = current + for_state.step_val;
                self.evaluator
                    .variables
                    .insert(for_state.variable.clone(), Value::Number(new_val));

                // Check if the loop continues
                let loop_continues = if for_state.step_val > 0.0 {
                    new_val <= for_state.end_val
                } else {
                    new_val >= for_state.end_val
                };

                if loop_continues {
                    // Jump back to the line after the FOR statement
                    Ok(StmtResult::Goto(program.lines[for_state.line_index + 1].line_number))
                } else {
                    // Remove the FOR state from the stack
                    if let Some(var_name) = variable {
                        if let Some(idx) = self.for_stack.iter().rposition(|f| f.variable == *var_name) {
                            self.for_stack.remove(idx);
                        }
                    } else {
                        self.for_stack.pop();
                    }
                    Ok(StmtResult::Continue)
                }
            }
            Statement::Rem(_) => Ok(StmtResult::Continue),
            Statement::End => Ok(StmtResult::End),
        }
    }

    /// Executes a PRINT statement. Commas advance to the next 14-character tab zone,
    /// semicolons suppress spacing, and a trailing separator suppresses the newline.
    pub(crate) fn execute_print(&mut self, items: &[PrintItem]) -> Result<(), String> {
        if items.is_empty() {
            writeln!(self.output).map_err(|e| e.to_string())?;
            self.column = 0;
            return Ok(());
        }

        let mut trailing_separator = false;
        for item in items {
            match item {
                PrintItem::Expression(expr) => {
                    let value = self.evaluator.eval_expr(expr)?;
                    let s = value.to_print_string();
                    write!(self.output, "{}", s).map_err(|e| e.to_string())?;
                    self.column += s.len();
                    trailing_separator = false;
                }
                PrintItem::Semicolon => {
                    trailing_separator = true;
                }
                PrintItem::Comma => {
                    // Tab to next 14-character zone
                    let next_tab = ((self.column / 14) + 1) * 14;
                    let spaces = next_tab - self.column;
                    write!(self.output, "{}", " ".repeat(spaces)).map_err(|e| e.to_string())?;
                    self.column = next_tab;
                    trailing_separator = true;
                }
            }
        }

        if !trailing_separator {
            writeln!(self.output).map_err(|e| e.to_string())?;
            self.column = 0;
        }

        Ok(())
    }

    /// Executes an INPUT statement: prints an optional prompt and "? ", reads a line,
    /// and stores it as a string (for $ variables) or parses it as a number.
    fn execute_input(&mut self, prompt: Option<&str>, variable: &str) -> Result<(), String> {
        if let Some(p) = prompt {
            write!(self.output, "{}", p).map_err(|e| e.to_string())?;
        }
        write!(self.output, "? ").map_err(|e| e.to_string())?;
        self.output.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        self.input.read_line(&mut line).map_err(|e| e.to_string())?;
        let line = line.trim().to_string();

        // Determine type based on variable name
        let value = if variable.ends_with('$') {
            Value::String(line)
        } else if let Ok(n) = line.parse::<f64>() {
            Value::Number(n)
        } else {
            Value::String(line)
        };

        self.evaluator.variables.insert(variable.to_string(), value);
        self.column = 0;
        Ok(())
    }

    /// Searches forward from a FOR statement to find its matching NEXT, respecting
    /// nesting depth of intervening FOR/NEXT pairs.
    fn find_matching_next(&self, program: &Program, for_line_idx: usize, var: &str) -> Result<usize, String> {
        let mut depth = 0;
        for i in (for_line_idx + 1)..program.lines.len() {
            for stmt in &program.lines[i].statements {
                match stmt {
                    Statement::For { .. } => depth += 1,
                    Statement::Next { variable } => {
                        if depth == 0 {
                            if let Some(v) = variable {
                                if v == var {
                                    return Ok(i);
                                }
                            } else {
                                return Ok(i);
                            }
                        } else {
                            depth -= 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(format!("No matching NEXT for FOR variable {}", var))
    }
}

/// Control flow result returned by statement execution.
pub(crate) enum StmtResult {
    Continue,
    Goto(u32),
    End,
    SkipLine,
    ForLoopSkip(usize),
}

/// Convenience function: parse and run a BASIC program from a string
#[cfg(test)]
pub fn run_program(source: &str) -> Result<String, String> {
    run_program_with_input(source, "")
}

/// Parse and run a BASIC program, providing input
#[cfg(test)]
pub fn run_program_with_input(source: &str, input: &str) -> Result<String, String> {
    use crate::ast::Parser;
    use crate::token::Lexer;
    use std::io;

    let tokens = Lexer::new(source).tokenize();
    let source_lines: Vec<String> = source.lines().map(String::from).collect();
    let mut parser = Parser::new(&tokens, source_lines);
    let program = parser.parse_program()?;

    let input_reader = io::Cursor::new(input.to_string());
    let mut output = Vec::new();

    {
        let mut interp = Interpreter::new(io::BufReader::new(input_reader), &mut output);
        interp.run(&program)?;
    }

    Ok(String::from_utf8(output).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_print() {
        let output = run_program("10 PRINT \"HELLO WORLD\"\n20 END\n").unwrap();
        assert_eq!(output, "HELLO WORLD\n");
    }

    #[test]
    fn test_print_number() {
        let output = run_program("10 PRINT 42\n20 END\n").unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_print_empty_line() {
        let output = run_program("10 PRINT\n20 END\n").unwrap();
        assert_eq!(output, "\n");
    }

    #[test]
    fn test_print_semicolon_suppresses_newline() {
        let output = run_program("10 PRINT \"A\";\n20 PRINT \"B\"\n30 END\n").unwrap();
        assert_eq!(output, "AB\n");
    }

    #[test]
    fn test_let_and_print() {
        let output = run_program("10 LET X = 5\n20 PRINT X\n30 END\n").unwrap();
        assert_eq!(output, " 5 \n");
    }

    #[test]
    fn test_implicit_let() {
        let output = run_program("10 X = 10\n20 PRINT X\n30 END\n").unwrap();
        assert_eq!(output, " 10 \n");
    }

    #[test]
    fn test_arithmetic() {
        let output = run_program("10 LET A = 10\n20 B = 20\n30 C = (A + B) * 2\n40 PRINT C\n50 END\n").unwrap();
        assert_eq!(output, " 60 \n");
    }

    #[test]
    fn test_goto() {
        let output = run_program("10 GOTO 30\n20 PRINT \"SKIP\"\n30 PRINT \"REACHED\"\n40 END\n").unwrap();
        assert_eq!(output, "REACHED\n");
    }

    #[test]
    fn test_if_then_true() {
        let output = run_program("10 LET X = 5\n20 IF X = 5 THEN PRINT \"YES\"\n30 END\n").unwrap();
        assert_eq!(output, "YES\n");
    }

    #[test]
    fn test_if_then_false() {
        let output = run_program("10 LET X = 3\n20 IF X = 5 THEN PRINT \"YES\"\n30 PRINT \"DONE\"\n40 END\n").unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_if_then_goto() {
        let output =
            run_program("10 LET X = 1\n20 IF X = 1 THEN GOTO 40\n30 PRINT \"SKIP\"\n40 PRINT \"JUMPED\"\n50 END\n")
                .unwrap();
        assert_eq!(output, "JUMPED\n");
    }

    #[test]
    fn test_for_next_loop() {
        let output = run_program("10 FOR I = 1 TO 3\n20 PRINT I;\n30 NEXT I\n40 END\n").unwrap();
        assert_eq!(output, " 1  2  3 ");
    }

    #[test]
    fn test_for_next_with_step() {
        let output = run_program("10 FOR I = 2 TO 10 STEP 2\n20 PRINT I;\n30 NEXT I\n40 END\n").unwrap();
        assert_eq!(output, " 2  4  6  8  10 ");
    }

    #[test]
    fn test_rem_ignored() {
        let output = run_program("10 REM THIS IS A COMMENT\n20 PRINT \"OK\"\n30 END\n").unwrap();
        assert_eq!(output, "OK\n");
    }

    #[test]
    fn test_input() {
        let output = run_program_with_input("10 INPUT N$\n20 PRINT \"HELLO \"; N$\n30 END\n", "ALICE\n").unwrap();
        assert_eq!(output, "? HELLO ALICE\n");
    }

    #[test]
    fn test_input_with_prompt() {
        let output = run_program_with_input("10 INPUT \"NAME: \"; N$\n20 PRINT N$\n30 END\n", "BOB\n").unwrap();
        assert_eq!(output, "NAME: ? BOB\n");
    }

    #[test]
    fn test_input_numeric() {
        let output = run_program_with_input("10 INPUT G\n20 PRINT G * 2\n30 END\n", "5\n").unwrap();
        assert_eq!(output, "?  10 \n");
    }

    #[test]
    fn test_counter_program() {
        let output = run_program(
            "\
10 REM COUNTER PROGRAM
20 LET X = 1
30 PRINT \"NUMBER:\"; X
40 X = X + 1
50 IF X <= 3 THEN GOTO 30
60 PRINT \"PROGRAM COMPLETE.\"
70 END
",
        )
        .unwrap();
        assert_eq!(output, "NUMBER: 1 \nNUMBER: 2 \nNUMBER: 3 \nPROGRAM COMPLETE.\n");
    }

    #[test]
    fn test_powers_of_2() {
        let output = run_program(
            "\
10 PRINT \"POWERS OF 2:\"
20 FOR I = 1 TO 5
30 PRINT 2 ^ I;
40 NEXT I
50 END
",
        )
        .unwrap();
        assert_eq!(output, "POWERS OF 2:\n 2  4  8  16  32 ");
    }

    #[test]
    fn test_conditional_math() {
        let output = run_program_with_input(
            "\
10 LET X = 5
20 INPUT \"GUESS (1-10): \"; G
30 IF G = X THEN PRINT \"CORRECT!\"
40 IF G <> X THEN PRINT \"WRONG, IT WAS\"; X
50 END
",
            "5\n",
        )
        .unwrap();
        assert_eq!(output, "GUESS (1-10): ? CORRECT!\n");
    }

    #[test]
    fn test_conditional_math_wrong() {
        let output = run_program_with_input(
            "\
10 LET X = 5
20 INPUT \"GUESS (1-10): \"; G
30 IF G = X THEN PRINT \"CORRECT!\"
40 IF G <> X THEN PRINT \"WRONG, IT WAS\"; X
50 END
",
            "3\n",
        )
        .unwrap();
        assert_eq!(output, "GUESS (1-10): ? WRONG, IT WAS 5 \n");
    }

    #[test]
    fn test_multi_statement_line() {
        let output = run_program("10 PRINT \"A\" : PRINT \"B\"\n20 END\n").unwrap();
        assert_eq!(output, "A\nB\n");
    }

    #[test]
    fn test_string_variable() {
        let output = run_program("10 LET N$ = \"WORLD\"\n20 PRINT \"HELLO \"; N$\n30 END\n").unwrap();
        assert_eq!(output, "HELLO WORLD\n");
    }

    #[test]
    fn test_for_loop_skip() {
        // Loop where start > end should skip the body
        let output =
            run_program("10 FOR I = 10 TO 1\n20 PRINT \"INSIDE\"\n30 NEXT I\n40 PRINT \"DONE\"\n50 END\n").unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_negative_step() {
        let output = run_program("10 FOR I = 5 TO 1 STEP -1\n20 PRINT I;\n30 NEXT I\n40 END\n").unwrap();
        assert_eq!(output, " 5  4  3  2  1 ");
    }

    #[test]
    fn test_print_expression() {
        let output = run_program("10 PRINT 2 + 3 * 4\n20 END\n").unwrap();
        assert_eq!(output, " 14 \n");
    }

    #[test]
    fn test_greeting_program() {
        let output = run_program_with_input(
            "\
10 PRINT \"WHAT IS YOUR NAME?\"
20 INPUT N$
30 PRINT \"HELLO \"; N$; \"!\"
40 END
",
            "ALICE\n",
        )
        .unwrap();
        assert_eq!(output, "WHAT IS YOUR NAME?\n? HELLO ALICE!\n");
    }

    #[test]
    fn test_counting_by_twos() {
        let output = run_program(
            "\
10 PRINT \"COUNTING BY TWOS:\"
20 FOR I = 2 TO 10 STEP 2
30 PRINT I;
40 NEXT I
50 PRINT \"DONE.\"
60 END
",
        )
        .unwrap();
        // After the FOR loop, PRINT I; leaves cursor at current position (no newline)
        // Then PRINT "DONE." prints on same line then newline
        assert_eq!(output, "COUNTING BY TWOS:\n 2  4  6  8  10 DONE.\n");
    }

    #[test]
    fn test_math_total() {
        let output = run_program(
            "\
10 LET A = 10
20 B = 20
30 C = (A + B) * 2
40 PRINT \"THE TOTAL IS:\"; C
50 END
",
        )
        .unwrap();
        assert_eq!(output, "THE TOTAL IS: 60 \n");
    }

    #[test]
    fn test_end_stops_execution() {
        let output = run_program("10 PRINT \"BEFORE\"\n20 END\n30 PRINT \"AFTER\"\n").unwrap();
        assert_eq!(output, "BEFORE\n");
    }

    #[test]
    fn test_empty_program() {
        let output = run_program("").unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_print_multiple_semicolons() {
        let output = run_program("10 PRINT \"A\"; \"B\"; \"C\"\n20 END\n").unwrap();
        assert_eq!(output, "ABC\n");
    }

    #[test]
    fn test_leading_whitespace_on_lines() {
        let output = run_program(
            "\
    10 PRINT \"A\"
    20 PRINT \"B\"
    30 END
",
        )
        .unwrap();
        assert_eq!(output, "A\nB\n");
    }

    #[test]
    fn test_empty_lines_in_program() {
        let output = run_program("\n10 PRINT \"A\"\n\n20 PRINT \"B\"\n\n30 END\n").unwrap();
        assert_eq!(output, "A\nB\n");
    }

    #[test]
    fn test_leading_whitespace_and_empty_lines() {
        let output = run_program("\n    10 LET X = 1\n\n    20 PRINT X\n\n    30 END\n").unwrap();
        assert_eq!(output, " 1 \n");
    }

    #[test]
    fn test_nested_for_loops() {
        let output = run_program(
            "\
10 FOR I = 1 TO 2
20 FOR J = 1 TO 2
30 PRINT I; J;
40 NEXT J
50 PRINT
60 NEXT I
70 END
",
        )
        .unwrap();
        assert_eq!(output, " 1  1  1  2 \n 2  1  2  2 \n");
    }

    #[test]
    fn test_if_then_else_true_branch() {
        let output = run_program("10 LET X = 5\n20 IF X = 5 THEN PRINT \"YES\" ELSE PRINT \"NO\"\n30 END\n").unwrap();
        assert_eq!(output, "YES\n");
    }

    #[test]
    fn test_if_then_else_false_branch() {
        let output = run_program("10 LET X = 3\n20 IF X = 5 THEN PRINT \"YES\" ELSE PRINT \"NO\"\n30 END\n").unwrap();
        assert_eq!(output, "NO\n");
    }

    #[test]
    fn test_if_then_else_with_goto() {
        let output = run_program(
            "\
10 LET X = 0
20 IF X = 1 THEN GOTO 50 ELSE GOTO 40
30 PRINT \"BAD\"
40 PRINT \"GOOD\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_if_then_else_with_line_numbers() {
        let output = run_program(
            "\
10 LET X = 0
20 IF X = 1 THEN 50 ELSE 40
30 PRINT \"BAD\"
40 PRINT \"GOOD\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_if_then_else_conditional_math() {
        let output = run_program_with_input(
            "\
10 LET X = 5
20 INPUT \"GUESS (1-10): \"; G
30 IF G = X THEN PRINT \"CORRECT!\" ELSE PRINT \"WRONG, IT WAS\"; X
40 END
",
            "3\n",
        )
        .unwrap();
        assert_eq!(output, "GUESS (1-10): ? WRONG, IT WAS 5 \n");
    }

    #[test]
    fn test_print_comma_tab_zones() {
        let output = run_program("10 PRINT \"A\", \"B\"\n20 END\n").unwrap();
        // "A" is 1 char, tab to column 14, then "B"
        assert_eq!(output, "A             B\n");
    }

    #[test]
    fn test_print_trailing_comma_suppresses_newline() {
        let output = run_program("10 PRINT \"A\",\n20 PRINT \"B\"\n30 END\n").unwrap();
        assert!(output.starts_with("A"));
        assert!(output.contains("B\n"));
    }

    #[test]
    fn test_if_then_line_number_truthy() {
        let output = run_program(
            "10 LET X = 1\n20 IF X = 1 THEN 40\n30 PRINT \"BAD\"\n40 PRINT \"GOOD\"\n50 END\n",
        )
        .unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_if_false_skips_remaining_statements_on_line() {
        // IF false should skip remaining statements on the same line (after colon)
        let output = run_program(
            "10 LET X = 0\n20 IF X = 1 THEN PRINT \"YES\"\n30 PRINT \"DONE\"\n40 END\n",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_goto_invalid_line_error() {
        let result = run_program("10 GOTO 999\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Line 999 not found"));
    }

    #[test]
    fn test_next_without_variable_uses_stack() {
        let output = run_program("10 FOR I = 1 TO 3\n20 PRINT I;\n30 NEXT\n40 END\n").unwrap();
        assert_eq!(output, " 1  2  3 ");
    }

    #[test]
    fn test_next_without_for_error() {
        let result = run_program("10 NEXT I\n20 END\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_next_without_for_no_variable_error() {
        let result = run_program("10 NEXT\n20 END\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_input_non_numeric_to_numeric_var() {
        // When a non-numeric string is input for a numeric variable, it stores as string
        let output = run_program_with_input(
            "10 INPUT G\n20 END\n",
            "HELLO\n",
        )
        .unwrap();
        assert_eq!(output, "? ");
    }

    #[test]
    fn test_for_loop_negative_step_skip() {
        // Negative step where start > end should skip
        let output = run_program(
            "10 FOR I = 1 TO 10 STEP -1\n20 PRINT \"INSIDE\"\n30 NEXT I\n40 PRINT \"DONE\"\n50 END\n",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_program_falls_off_end() {
        // Program with no END statement just finishes
        let output = run_program("10 PRINT \"HI\"\n").unwrap();
        assert_eq!(output, "HI\n");
    }

    #[test]
    fn test_nested_for_skip() {
        // Skip a FOR loop that has a nested FOR inside
        let output = run_program(
            "\
10 FOR I = 10 TO 1
20 FOR J = 1 TO 3
30 PRINT J
40 NEXT J
50 NEXT I
60 PRINT \"DONE\"
70 END
",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_for_loop_no_matching_next_error() {
        let result = run_program("10 FOR I = 10 TO 1\n20 PRINT \"HI\"\n30 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No matching NEXT"));
    }

    #[test]
    fn test_apostrophe_comment_in_program() {
        let output = run_program("10 ' THIS IS A COMMENT\n20 PRINT \"OK\"\n30 END\n").unwrap();
        assert_eq!(output, "OK\n");
    }

    #[test]
    fn test_runtime_error_includes_context() {
        let result = run_program("10 PRINT UNDEFINED_VAR\n20 END\n");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("at line"));
    }

    #[test]
    fn test_if_else_line_number_falsy() {
        let output = run_program(
            "10 LET X = 0\n20 IF X = 1 THEN 50 ELSE 40\n30 PRINT \"BAD\"\n40 PRINT \"GOOD\"\n50 END\n",
        )
        .unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_for_skip_with_next_no_variable() {
        // FOR loop skipped, and matching NEXT has no variable
        let output = run_program(
            "10 FOR I = 10 TO 1\n20 PRINT \"INSIDE\"\n30 NEXT\n40 PRINT \"DONE\"\n50 END\n",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_for_loop_completes_without_variable_in_next() {
        // NEXT without variable name uses stack pop to end loop
        let output = run_program("10 FOR I = 1 TO 2\n20 PRINT I;\n30 NEXT\n40 END\n").unwrap();
        assert_eq!(output, " 1  2 ");
    }

    #[test]
    fn test_for_skip_nested_with_different_next_variable() {
        // Outer FOR I skipped. Inner has NEXT J (different var), then NEXT I matches.
        let output = run_program(
            "\
10 FOR I = 10 TO 1
20 FOR J = 1 TO 3
30 NEXT J
40 NEXT I
50 PRINT \"DONE\"
60 END
",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }
}
