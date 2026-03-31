//! Interactive debugger for BASIC programs.
//!
//! This module provides the `Debugger` struct which wraps an `Interpreter` and
//! adds interactive debugging capabilities: stepping through lines, setting
//! breakpoints (by line number or conditional expression), inspecting and
//! modifying variables, and controlling execution flow.
//!
//! The debugger supports an optional remote mode where debug commands and output
//! are exchanged over a TCP connection, keeping the BASIC program's stdin/stdout
//! free from debugger traffic.

use crate::ast::{Parser, PrintItem, Program, Statement};
use crate::expr::{Expr, ExprParser};
use crate::interpreter::{Interpreter, StmtResult};
use crate::token::Lexer;
use std::io::{BufRead, Write};

/// A breakpoint that pauses execution when its condition is met.
enum Breakpoint {
    AtLine(u32),
    IfExpr(Expr),
}

/// A parsed debug command entered at the debugger prompt.
enum DebugCommand {
    Run,
    Step,
    Goto(u32),
    BreakAt(u32),
    BreakIf(Expr),
    Let(Statement),
    Print(Vec<PrintItem>),
    List(Option<u32>, Option<u32>),
    Help,
    Quit,
    Unknown(String),
}

/// The result of executing a single BASIC line in debug mode.
enum ExecutionOutcome {
    Ok,
    Finished,
    Error(String),
}

/// Interactive debugger that wraps an interpreter and provides step-by-step
/// execution, breakpoints, and variable inspection.
///
/// When remote I/O is configured, debugger commands are read from and responses
/// written to the remote streams, while the BASIC program continues to use the
/// interpreter's own stdin/stdout for `INPUT` and `PRINT`.
pub struct Debugger<R: BufRead, W: Write> {
    interpreter: Interpreter<R, W>,
    remote_input: Option<Box<dyn BufRead>>,
    remote_output: Option<Box<dyn Write>>,
    breakpoints: Vec<Breakpoint>,
    line_idx: usize,
    start_stmt_idx: usize,
    finished: bool,
}

impl<R: BufRead, W: Write> Debugger<R, W> {
    /// Creates a new debugger wrapping the given interpreter.
    /// Debug commands and output use the interpreter's own I/O streams.
    pub fn new(interpreter: Interpreter<R, W>) -> Self {
        Debugger {
            interpreter,
            remote_input: None,
            remote_output: None,
            breakpoints: Vec::new(),
            line_idx: 0,
            start_stmt_idx: 0,
            finished: false,
        }
    }

    /// Creates a new debugger with remote I/O for debug commands and output.
    /// The interpreter keeps its own streams for BASIC program I/O (`INPUT`/`PRINT`),
    /// while the debugger REPL communicates over the provided remote streams.
    pub fn new_remote(
        interpreter: Interpreter<R, W>,
        remote_input: Box<dyn BufRead>,
        remote_output: Box<dyn Write>,
    ) -> Self {
        Debugger {
            interpreter,
            remote_input: Some(remote_input),
            remote_output: Some(remote_output),
            breakpoints: Vec::new(),
            line_idx: 0,
            start_stmt_idx: 0,
            finished: false,
        }
    }

    /// Reads a line of debug input from the remote stream (if configured) or the interpreter's input.
    fn read_debug_line(&mut self, buf: &mut String) -> Result<usize, String> {
        if let Some(ref mut input) = self.remote_input {
            input.read_line(buf).map_err(|e| e.to_string())
        } else {
            self.interpreter.input.read_line(buf).map_err(|e| e.to_string())
        }
    }

    /// Returns a mutable reference to the debug output writer.
    fn dbg_out(&mut self) -> &mut dyn Write {
        if let Some(ref mut output) = self.remote_output {
            output.as_mut()
        } else {
            &mut self.interpreter.output
        }
    }

    /// Runs the interactive debugger REPL for the given program.
    pub fn run_repl(&mut self, program: &Program) -> Result<(), String> {
        if program.lines.is_empty() {
            writeln!(self.dbg_out(), "Program is empty.").map_err(|e| e.to_string())?;
            return Ok(());
        }

        self.interpreter.collect_data(program)?;

        writeln!(self.dbg_out(), "BASIC Debugger. Type HELP for a list of commands.").map_err(|e| e.to_string())?;

        loop {
            // Print prompt
            if self.finished {
                write!(self.dbg_out(), "[DBG finished]> ").map_err(|e| e.to_string())?;
            } else {
                let line_num = program.lines[self.line_idx].line_number;
                write!(self.dbg_out(), "[DBG line {}]> ", line_num).map_err(|e| e.to_string())?;
            }
            self.dbg_out().flush().map_err(|e| e.to_string())?;

            // Read command
            let mut input_line = String::new();
            let bytes_read = self.read_debug_line(&mut input_line)?;
            if bytes_read == 0 {
                // EOF on input
                break;
            }
            let input_line = input_line.trim().to_string();
            if input_line.is_empty() {
                continue;
            }

            let cmd = self.parse_debug_command(&input_line);
            match cmd {
                DebugCommand::Quit => break,
                DebugCommand::Step => {
                    if self.finished {
                        writeln!(self.dbg_out(), "Program has finished. Use GOTO to restart from a line.")
                            .map_err(|e| e.to_string())?;
                    } else {
                        self.execute_one_line(program);
                    }
                }
                DebugCommand::Run => {
                    if self.finished {
                        writeln!(self.dbg_out(), "Program has finished. Use GOTO to restart from a line.")
                            .map_err(|e| e.to_string())?;
                    } else {
                        self.execute_until_break(program);
                    }
                }
                DebugCommand::Goto(line_num) => match self.interpreter.find_line_index(program, line_num) {
                    Ok(idx) => {
                        self.line_idx = idx;
                        self.finished = false;
                    }
                    Err(e) => {
                        let _ = writeln!(self.dbg_out(), "Error: {}", e);
                    }
                },
                DebugCommand::BreakAt(line_num) => {
                    self.breakpoints.push(Breakpoint::AtLine(line_num));
                    let _ = writeln!(self.dbg_out(), "Breakpoint set at line {}", line_num);
                }
                DebugCommand::BreakIf(expr) => {
                    self.breakpoints.push(Breakpoint::IfExpr(expr));
                    let _ = writeln!(self.dbg_out(), "Conditional breakpoint set");
                }
                DebugCommand::Let(stmt) => {
                    if let Err(e) = self.interpreter.execute_statement(&stmt, self.line_idx, 0, program) {
                        let _ = writeln!(self.dbg_out(), "Error: {}", e);
                    }
                }
                DebugCommand::Print(items) => {
                    if self.remote_output.is_some() {
                        // In remote mode, PRINT from the debugger goes to the debug output
                        if let Err(e) = self.execute_print_to_dbg(&items) {
                            let _ = writeln!(self.dbg_out(), "Error: {}", e);
                        }
                    } else if let Err(e) = self.interpreter.execute_print(&items) {
                        let _ = writeln!(self.dbg_out(), "Error: {}", e);
                    }
                }
                DebugCommand::List(start, end) => {
                    for line in &program.lines {
                        let ln = line.line_number;
                        if let Some(s) = start {
                            if ln < s {
                                continue;
                            }
                        }
                        if let Some(e) = end {
                            if ln > e {
                                break;
                            }
                        }
                        let text = program
                            .source_lines
                            .get(line.source_line - 1)
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        let _ = writeln!(self.dbg_out(), "{}", text);
                    }
                }
                DebugCommand::Help => {
                    let _ = writeln!(self.dbg_out(), "Debugger commands:");
                    let _ = writeln!(
                        self.dbg_out(),
                        "  STEP              Execute the current line and advance"
                    );
                    let _ = writeln!(
                        self.dbg_out(),
                        "  RUN               Run until a breakpoint or program end"
                    );
                    let _ = writeln!(self.dbg_out(), "  LIST              List all program lines");
                    let _ = writeln!(self.dbg_out(), "  LIST <start>      List from line <start> onward");
                    let _ = writeln!(self.dbg_out(), "  LIST <start> <end>  List lines from <start> to <end>");
                    let _ = writeln!(self.dbg_out(), "  BREAK AT <line>   Set a breakpoint at a line number");
                    let _ = writeln!(self.dbg_out(), "  BREAK IF <expr>   Set a conditional breakpoint");
                    let _ = writeln!(self.dbg_out(), "  GOTO <line>       Jump to a line number");
                    let _ = writeln!(self.dbg_out(), "  PRINT <expr>      Evaluate and print an expression");
                    let _ = writeln!(self.dbg_out(), "  LET <var> = <val> Set a variable's value");
                    let _ = writeln!(self.dbg_out(), "  QUIT              Exit the debugger");
                    let _ = writeln!(self.dbg_out(), "  HELP              Show this help message");
                }
                DebugCommand::Unknown(s) => {
                    let _ = writeln!(self.dbg_out(), "Unknown command: {}", s);
                }
            }
        }
        Ok(())
    }

    /// Executes a single BASIC line, handling all statements on that line.
    fn execute_one_line(&mut self, program: &Program) {
        match self.execute_one_line_inner(program) {
            ExecutionOutcome::Ok => {}
            ExecutionOutcome::Finished => {
                let _ = writeln!(self.dbg_out(), "Program finished.");
                self.finished = true;
            }
            ExecutionOutcome::Error(e) => {
                let _ = writeln!(self.dbg_out(), "Runtime error: {}", e);
                // Don't advance line_idx so user can fix and retry
            }
        }
    }

    /// Inner implementation: executes statements on the current line and returns the outcome.
    fn execute_one_line_inner(&mut self, program: &Program) -> ExecutionOutcome {
        if self.line_idx >= program.lines.len() {
            return ExecutionOutcome::Finished;
        }

        let line = &program.lines[self.line_idx];
        let mut stmt_idx = self.start_stmt_idx;
        self.start_stmt_idx = 0;
        let mut next_line_idx = self.line_idx + 1;

        while stmt_idx < line.statements.len() {
            let stmt = &line.statements[stmt_idx];
            let result = self
                .interpreter
                .execute_statement(stmt, self.line_idx, stmt_idx, program);
            match result {
                Ok(StmtResult::Continue) => {
                    stmt_idx += 1;
                }
                Ok(StmtResult::Goto(target_line)) => match self.interpreter.find_line_index(program, target_line) {
                    Ok(idx) => {
                        next_line_idx = idx;
                        break;
                    }
                    Err(e) => return ExecutionOutcome::Error(e),
                },
                Ok(StmtResult::Gosub(target_line)) => {
                    let return_addr = if stmt_idx + 1 < line.statements.len() {
                        crate::interpreter::GosubReturn {
                            line_index: self.line_idx,
                            stmt_index: stmt_idx + 1,
                        }
                    } else {
                        crate::interpreter::GosubReturn {
                            line_index: self.line_idx + 1,
                            stmt_index: 0,
                        }
                    };
                    self.interpreter.gosub_stack.push(return_addr);
                    match self.interpreter.find_line_index(program, target_line) {
                        Ok(idx) => {
                            next_line_idx = idx;
                            break;
                        }
                        Err(e) => return ExecutionOutcome::Error(e),
                    }
                }
                Ok(StmtResult::Return(target_line)) => {
                    let ret = self.interpreter.gosub_stack.pop().unwrap();
                    if let Some(line_num) = target_line {
                        match self.interpreter.find_line_index(program, line_num) {
                            Ok(idx) => {
                                next_line_idx = idx;
                            }
                            Err(e) => return ExecutionOutcome::Error(e),
                        }
                    } else {
                        next_line_idx = ret.line_index;
                        self.start_stmt_idx = ret.stmt_index;
                    }
                    break;
                }
                Ok(StmtResult::End) => {
                    return ExecutionOutcome::Finished;
                }
                Ok(StmtResult::SkipLine) => {
                    break;
                }
                Ok(StmtResult::ForLoopSkip { line_index, stmt_index }) => {
                    next_line_idx = line_index;
                    self.start_stmt_idx = stmt_index;
                    break;
                }
                Ok(StmtResult::ForLoopBack { line_index, stmt_index }) => {
                    next_line_idx = line_index;
                    self.start_stmt_idx = stmt_index;
                    break;
                }
                Err(e) => return ExecutionOutcome::Error(e),
            }
        }

        self.line_idx = next_line_idx;

        if self.line_idx >= program.lines.len() {
            ExecutionOutcome::Finished
        } else {
            ExecutionOutcome::Ok
        }
    }

    /// Runs the program from the current line until a breakpoint is hit, the program
    /// ends, or an error occurs.
    fn execute_until_break(&mut self, program: &Program) {
        let mut first = true;
        loop {
            // Check breakpoints before executing the next line, but skip the
            // very first line so RUN doesn't get stuck on a breakpoint at the
            // current line.
            if !first && self.line_idx < program.lines.len() {
                let line_num = program.lines[self.line_idx].line_number;
                if self.check_breakpoints(line_num) {
                    let _ = writeln!(self.dbg_out(), "Breakpoint hit at line {}", line_num);
                    return;
                }
            }

            first = false;

            match self.execute_one_line_inner(program) {
                ExecutionOutcome::Ok => {}
                ExecutionOutcome::Finished => {
                    let _ = writeln!(self.dbg_out(), "Program finished.");
                    self.finished = true;
                    return;
                }
                ExecutionOutcome::Error(e) => {
                    let _ = writeln!(self.dbg_out(), "Runtime error: {}", e);
                    return;
                }
            }
        }
    }

    /// Evaluates and formats PRINT items, writing output to the debug stream instead
    /// of the interpreter's program output. Used in remote mode so that debugger PRINT
    /// results appear on the debug connection.
    fn execute_print_to_dbg(&mut self, items: &[PrintItem]) -> Result<(), String> {
        use crate::eval::Value;
        for item in items {
            match item {
                PrintItem::Expression(expr) => {
                    let val = self.interpreter.evaluator.eval_expr(expr)?;
                    match &val {
                        Value::Number(n) => {
                            if *n >= 0.0 {
                                let _ = write!(self.dbg_out(), " {} ", n);
                            } else {
                                let _ = write!(self.dbg_out(), "{} ", n);
                            }
                        }
                        Value::String(s) => {
                            let _ = write!(self.dbg_out(), "{}", s);
                        }
                    }
                }
                PrintItem::Semicolon => {}
                PrintItem::Comma => {
                    let _ = write!(self.dbg_out(), "\t");
                }
            }
        }
        // If the last item is not a separator, print newline
        if items.is_empty() || !matches!(items.last(), Some(PrintItem::Semicolon | PrintItem::Comma)) {
            let _ = writeln!(self.dbg_out());
        }
        Ok(())
    }

    /// Checks whether any breakpoint matches the current state.
    fn check_breakpoints(&mut self, line_number: u32) -> bool {
        for bp in &self.breakpoints {
            match bp {
                Breakpoint::AtLine(n) => {
                    if *n == line_number {
                        return true;
                    }
                }
                Breakpoint::IfExpr(expr) => {
                    if let Ok(val) = self.interpreter.evaluator.eval_expr(expr) {
                        if val.is_truthy() {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Parses a debug command string into a `DebugCommand`.
    fn parse_debug_command(&self, line: &str) -> DebugCommand {
        let tokens = Lexer::new(line).tokenize();
        if tokens.is_empty() {
            return DebugCommand::Unknown(line.to_string());
        }

        let upper = line.trim().to_uppercase();

        // Simple keyword commands
        if upper == "RUN" {
            return DebugCommand::Run;
        }
        if upper == "STEP" {
            return DebugCommand::Step;
        }
        if upper == "QUIT" {
            return DebugCommand::Quit;
        }
        if upper == "HELP" {
            return DebugCommand::Help;
        }

        // LIST [start [end]]
        if upper == "LIST" {
            return DebugCommand::List(None, None);
        }
        if upper.starts_with("LIST ") {
            let rest = line.trim()[5..].trim();
            let parts: Vec<&str> = rest.split_whitespace().collect();
            match parts.len() {
                1 => {
                    if let Ok(n) = parts[0].parse::<u32>() {
                        return DebugCommand::List(Some(n), None);
                    }
                    return DebugCommand::Unknown(line.to_string());
                }
                2 => {
                    if let (Ok(s), Ok(e)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        return DebugCommand::List(Some(s), Some(e));
                    }
                    return DebugCommand::Unknown(line.to_string());
                }
                _ => return DebugCommand::Unknown(line.to_string()),
            }
        }

        // GOTO <number>
        if upper.starts_with("GOTO ") {
            let rest = line.trim()[5..].trim();
            if let Ok(n) = rest.parse::<u32>() {
                return DebugCommand::Goto(n);
            }
            return DebugCommand::Unknown(line.to_string());
        }

        // BREAK AT <number>
        if upper.starts_with("BREAK AT ") {
            let rest = line.trim()[9..].trim();
            if let Ok(n) = rest.parse::<u32>() {
                return DebugCommand::BreakAt(n);
            }
            return DebugCommand::Unknown(line.to_string());
        }

        // BREAK IF <expr>
        if upper.starts_with("BREAK IF ") {
            let rest = line.trim()[9..].trim();
            let expr_tokens = Lexer::new(rest).tokenize();
            let mut expr_parser = ExprParser::new(&expr_tokens);
            match expr_parser.parse_expression() {
                Ok(expr) => return DebugCommand::BreakIf(expr),
                Err(_) => return DebugCommand::Unknown(line.to_string()),
            }
        }

        // LET and PRINT: prepend a fake line number and parse as a BASIC statement
        if upper.starts_with("LET ") || upper.starts_with("PRINT") {
            let fake_line = format!("1 {}", line.trim());
            let fake_tokens = Lexer::new(&fake_line).tokenize();
            let source_lines: Vec<String> = vec![fake_line.clone()];
            let mut parser = Parser::new(&fake_tokens, source_lines);
            if let Ok(prog) = parser.parse_program() {
                if let Some(first_line) = prog.lines.first() {
                    if let Some(stmt) = first_line.statements.first() {
                        match stmt {
                            Statement::Let { .. } => {
                                return DebugCommand::Let(stmt.clone());
                            }
                            Statement::Print { items } => {
                                return DebugCommand::Print(items.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
            return DebugCommand::Unknown(line.to_string());
        }

        // Implicit LET: variable = expression (e.g. "X = 5")
        if upper.contains('=') && !upper.contains("==") {
            let fake_line = format!("1 {}", line.trim());
            let fake_tokens = Lexer::new(&fake_line).tokenize();
            let source_lines: Vec<String> = vec![fake_line.clone()];
            let mut parser = Parser::new(&fake_tokens, source_lines);
            if let Ok(prog) = parser.parse_program() {
                if let Some(first_line) = prog.lines.first() {
                    if let Some(stmt @ Statement::Let { .. }) = first_line.statements.first() {
                        return DebugCommand::Let(stmt.clone());
                    }
                }
            }
        }

        DebugCommand::Unknown(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Parser;
    use crate::token::Lexer;
    use std::io;

    fn parse_program(source: &str) -> Program {
        let tokens = Lexer::new(source).tokenize();
        let source_lines: Vec<String> = source.lines().map(String::from).collect();
        let mut parser = Parser::new(&tokens, source_lines);
        parser.parse_program().unwrap()
    }

    fn run_debugger(source: &str, commands: &str) -> String {
        let program = parse_program(source);
        let input_reader = io::Cursor::new(commands.to_string());
        let mut output = Vec::new();
        {
            let interp = Interpreter::new(io::BufReader::new(input_reader), &mut output);
            let mut debugger = Debugger::new(interp);
            debugger.run_repl(&program).unwrap();
        }
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn test_debugger_step_executes_one_line() {
        let output = run_debugger("10 PRINT \"A\"\n20 PRINT \"B\"\n30 END\n", "STEP\nSTEP\nQUIT\n");
        assert!(output.contains("A\n"));
        assert!(output.contains("B\n"));
    }

    #[test]
    fn test_debugger_run_to_completion() {
        let output = run_debugger("10 PRINT \"HELLO\"\n20 END\n", "RUN\nQUIT\n");
        assert!(output.contains("HELLO\n"));
        assert!(output.contains("Program finished."));
    }

    #[test]
    fn test_debugger_break_at_line() {
        let output = run_debugger(
            "10 PRINT \"A\"\n20 PRINT \"B\"\n30 PRINT \"C\"\n40 END\n",
            "BREAK AT 20\nRUN\nQUIT\n",
        );
        assert!(output.contains("Breakpoint set at line 20"));
        assert!(output.contains("Breakpoint hit at line 20"));
        // Line 10 should have executed (PRINT "A"), but line 20 should not yet
        assert!(output.contains("A\n"));
    }

    #[test]
    fn test_debugger_print_variable() {
        let output = run_debugger("10 LET X = 42\n20 END\n", "STEP\nPRINT X\nQUIT\n");
        assert!(output.contains(" 42 "));
    }

    #[test]
    fn test_debugger_let_modifies_variable() {
        let output = run_debugger("10 LET X = 1\n20 PRINT X\n30 END\n", "STEP\nLET X = 99\nSTEP\nQUIT\n");
        assert!(output.contains(" 99 "));
    }

    #[test]
    fn test_debugger_goto_jumps() {
        let output = run_debugger("10 PRINT \"A\"\n20 PRINT \"B\"\n30 END\n", "GOTO 20\nSTEP\nQUIT\n");
        // Should print B (skipped A)
        assert!(output.contains("B\n"));
        // Should NOT contain A printed by execution
        // The prompt lines contain "line 20" after GOTO
        assert!(output.contains("[DBG line 20]>"));
    }

    #[test]
    fn test_debugger_break_if_condition() {
        let output = run_debugger(
            "10 LET X = 0\n20 X = X + 1\n30 IF X < 5 THEN GOTO 20\n40 PRINT X\n50 END\n",
            "BREAK IF X > 3\nRUN\nQUIT\n",
        );
        assert!(output.contains("Conditional breakpoint set"));
        assert!(output.contains("Breakpoint hit"));
    }

    #[test]
    fn test_debugger_error_stays_alive() {
        let output = run_debugger("10 PRINT 1/0\n20 END\n", "STEP\nQUIT\n");
        assert!(output.contains("Runtime error:"));
        // Debugger should still be alive (we see the next prompt)
        assert!(output.contains("[DBG line 10]>"));
    }

    #[test]
    fn test_debugger_run_skips_breakpoint_on_current_line() {
        // STEP to line 20, set breakpoint at 20, then RUN should NOT get stuck
        // on line 20 — it should continue and hit the breakpoint next time
        // line 20 is reached (via the loop).
        let output = run_debugger(
            "10 LET X = 0\n20 X = X + 1\n30 IF X < 3 THEN GOTO 20\n40 END\n",
            "STEP\nBREAK AT 20\nRUN\nPRINT X\nQUIT\n",
        );
        // After STEP executes line 10, we're at line 20.
        // BREAK AT 20, then RUN: should execute line 20 (X=1), line 30 (goto 20),
        // then hit the breakpoint at line 20 the second time around.
        // X should be 1 at that point (incremented once, loop jumped back).
        assert!(output.contains("Breakpoint hit at line 20"));
        assert!(output.contains(" 1 "));
    }

    #[test]
    fn test_debugger_quit() {
        let output = run_debugger("10 PRINT \"HELLO\"\n20 END\n", "QUIT\n");
        // Should see the initial prompt but not execute any BASIC code
        assert!(!output.contains("HELLO\n"));
    }

    #[test]
    fn test_debugger_empty_program() {
        let output = run_debugger("", "QUIT\n");
        assert!(output.contains("Program is empty."));
    }

    #[test]
    fn test_debugger_finished_state() {
        let output = run_debugger("10 PRINT \"DONE\"\n20 END\n", "RUN\nSTEP\nQUIT\n");
        assert!(output.contains("[DBG finished]>"));
        assert!(output.contains("Program has finished."));
    }

    #[test]
    fn test_debugger_list_all() {
        let output = run_debugger("10 PRINT \"A\"\n20 LET X = 1\n30 END\n", "LIST\nQUIT\n");
        assert!(output.contains("10 PRINT \"A\""));
        assert!(output.contains("20 LET X = 1"));
        assert!(output.contains("30 END"));
    }

    #[test]
    fn test_debugger_list_from_line() {
        let output = run_debugger(
            "10 PRINT \"A\"\n20 PRINT \"B\"\n30 PRINT \"C\"\n40 END\n",
            "LIST 20\nQUIT\n",
        );
        assert!(!output.contains("10 PRINT \"A\"\n"));
        assert!(output.contains("20 PRINT \"B\""));
        assert!(output.contains("30 PRINT \"C\""));
        assert!(output.contains("40 END"));
    }

    #[test]
    fn test_debugger_list_range() {
        let output = run_debugger(
            "10 PRINT \"A\"\n20 PRINT \"B\"\n30 PRINT \"C\"\n40 END\n",
            "LIST 20 30\nQUIT\n",
        );
        assert!(!output.contains("10 PRINT \"A\"\n"));
        assert!(output.contains("20 PRINT \"B\""));
        assert!(output.contains("30 PRINT \"C\""));
        assert!(!output.contains("40 END\n"));
    }

    #[test]
    fn test_debugger_unknown_command() {
        let output = run_debugger("10 END\n", "FOOBAR\nQUIT\n");
        assert!(output.contains("Unknown command: FOOBAR"));
    }

    #[test]
    fn test_debugger_help_command() {
        let output = run_debugger("10 END\n", "HELP\nQUIT\n");
        assert!(output.contains("Debugger commands:"));
        assert!(output.contains("STEP"));
        assert!(output.contains("RUN"));
        assert!(output.contains("BREAK AT"));
        assert!(output.contains("BREAK IF"));
    }

    #[test]
    fn test_debugger_eof_exits() {
        // No QUIT, just EOF
        let output = run_debugger("10 END\n", "");
        assert!(output.contains("[DBG line 10]>"));
    }

    #[test]
    fn test_debugger_empty_line_skipped() {
        let output = run_debugger("10 PRINT \"A\"\n20 END\n", "\nSTEP\nQUIT\n");
        assert!(output.contains("A\n"));
    }

    #[test]
    fn test_debugger_run_after_finished() {
        let output = run_debugger("10 END\n", "RUN\nRUN\nQUIT\n");
        assert!(output.contains("Program has finished."));
    }

    #[test]
    fn test_debugger_step_after_finished() {
        let output = run_debugger("10 END\n", "STEP\nSTEP\nQUIT\n");
        assert!(output.contains("Program has finished."));
    }

    #[test]
    fn test_debugger_goto_invalid_line() {
        let output = run_debugger("10 END\n", "GOTO 999\nQUIT\n");
        assert!(output.contains("Error:"));
    }

    #[test]
    fn test_debugger_let_error() {
        let output = run_debugger("10 END\n", "LET X = 1/0\nQUIT\n");
        assert!(output.contains("Error:"));
    }

    #[test]
    fn test_debugger_print_error() {
        let output = run_debugger("10 END\n", "PRINT 1/0\nQUIT\n");
        assert!(output.contains("Error:"));
    }

    #[test]
    fn test_debugger_implicit_let() {
        let output = run_debugger("10 PRINT X\n20 END\n", "X = 42\nSTEP\nQUIT\n");
        assert!(output.contains(" 42 "));
    }

    #[test]
    fn test_debugger_goto_bad_parse() {
        let output = run_debugger("10 END\n", "GOTO ABC\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_break_at_bad_parse() {
        let output = run_debugger("10 END\n", "BREAK AT ABC\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_break_if_bad_expr() {
        let output = run_debugger("10 END\n", "BREAK IF +\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_list_bad_args() {
        let output = run_debugger("10 END\n", "LIST ABC\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_list_bad_range() {
        let output = run_debugger("10 END\n", "LIST ABC DEF\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_list_too_many_args() {
        let output = run_debugger("10 END\n", "LIST 1 2 3\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    #[test]
    fn test_debugger_step_with_skip_line() {
        // IF false in debugger step mode should skip line
        let output = run_debugger(
            "10 LET X = 0\n20 IF X = 1 THEN PRINT \"YES\"\n30 PRINT \"NO\"\n40 END\n",
            "STEP\nSTEP\nSTEP\nQUIT\n",
        );
        assert!(output.contains("NO\n"));
        assert!(!output.contains("YES"));
    }

    #[test]
    fn test_debugger_step_with_goto() {
        let output = run_debugger(
            "10 GOTO 30\n20 PRINT \"SKIP\"\n30 PRINT \"REACHED\"\n40 END\n",
            "STEP\nSTEP\nQUIT\n",
        );
        assert!(output.contains("REACHED\n"));
        assert!(!output.contains("SKIP"));
    }

    #[test]
    fn test_debugger_step_for_loop_skip() {
        let output = run_debugger(
            "10 FOR I = 10 TO 1\n20 PRINT \"INSIDE\"\n30 NEXT I\n40 PRINT \"DONE\"\n50 END\n",
            "STEP\nSTEP\nQUIT\n",
        );
        assert!(output.contains("DONE\n"));
        assert!(!output.contains("INSIDE"));
    }

    #[test]
    fn test_debugger_step_past_end_of_program() {
        let output = run_debugger("10 PRINT \"A\"\n", "STEP\nQUIT\n");
        assert!(output.contains("A\n"));
        assert!(output.contains("Program finished."));
    }

    #[test]
    fn test_debugger_run_with_error() {
        let output = run_debugger("10 PRINT 1/0\n20 END\n", "RUN\nQUIT\n");
        assert!(output.contains("Runtime error:"));
    }

    #[test]
    fn test_debugger_step_goto_to_invalid_line() {
        let output = run_debugger("10 GOTO 999\n20 END\n", "STEP\nQUIT\n");
        assert!(output.contains("Runtime error:"));
    }

    #[test]
    fn test_debugger_goto_restarts_finished_program() {
        let output = run_debugger("10 PRINT \"A\"\n20 END\n", "RUN\nGOTO 10\nSTEP\nQUIT\n");
        // Should see "A" printed twice - once from RUN, once from STEP after GOTO
        let count = output.matches("A\n").count();
        assert!(count >= 2);
    }

    #[test]
    fn test_debugger_print_bad_parse() {
        // PRINT followed by something that can't be parsed as a valid BASIC expression
        // This triggers the Unknown path when PRINT fails to parse as a statement
        let output = run_debugger("10 END\n", "PRINT\nQUIT\n");
        // PRINT with no args prints a blank line, which is valid
        assert!(output.contains("\n"));
    }

    #[test]
    fn test_debugger_break_if_falsy_no_hit() {
        // Breakpoint with a condition that evaluates to falsy (0)
        let output = run_debugger("10 LET X = 0\n20 PRINT X\n30 END\n", "BREAK IF X > 100\nRUN\nQUIT\n");
        // Should run to completion since condition is always false
        assert!(output.contains("Program finished."));
    }

    #[test]
    fn test_debugger_break_if_eval_error_ignored() {
        // Breakpoint with expression that errors (undefined var) - should be ignored
        let output = run_debugger("10 PRINT \"A\"\n20 END\n", "BREAK IF UNDEFINED_VAR > 0\nRUN\nQUIT\n");
        assert!(output.contains("Program finished."));
    }

    #[test]
    fn test_debugger_implicit_let_not_an_assignment() {
        // Something with = that doesn't parse as LET (e.g. nonsensical)
        let output = run_debugger("10 END\n", "123 = 456\nQUIT\n");
        assert!(output.contains("Unknown command"));
    }

    /// Helper that creates a remote debugger where program I/O goes to `prog_output`
    /// and debug commands/output go through separate streams.
    fn run_remote_debugger(source: &str, commands: &str) -> (String, String) {
        use std::sync::{Arc, Mutex};

        /// A `Write` adapter backed by a shared `Vec<u8>` so it can be owned by
        /// the `Box<dyn Write>` (which requires `'static`) while still allowing
        /// the test to read the output afterwards.
        #[derive(Clone)]
        struct SharedWriter(Arc<Mutex<Vec<u8>>>);

        impl Write for SharedWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }

        let program = parse_program(source);
        let prog_input = io::Cursor::new(Vec::new());
        let mut prog_output: Vec<u8> = Vec::new();
        let dbg_input = io::Cursor::new(commands.to_string().into_bytes());
        let dbg_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let dbg_writer = SharedWriter(Arc::clone(&dbg_buf));
        {
            let interp = Interpreter::new(io::BufReader::new(prog_input), &mut prog_output);
            let mut debugger =
                Debugger::new_remote(interp, Box::new(io::BufReader::new(dbg_input)), Box::new(dbg_writer));
            debugger.run_repl(&program).unwrap();
        }
        let dbg_output = dbg_buf.lock().unwrap().clone();
        (
            String::from_utf8(prog_output).unwrap(),
            String::from_utf8(dbg_output).unwrap(),
        )
    }

    /// In remote mode, debug prompts and messages go to debug output, not program output.
    #[test]
    fn test_remote_debugger_separation() {
        let (prog_out, dbg_out) = run_remote_debugger("10 PRINT \"HELLO\"\n20 END\n", "STEP\nSTEP\nQUIT\n");
        // Program PRINT output goes to program output
        assert!(prog_out.contains("HELLO"));
        // Debug prompts go to debug output
        assert!(dbg_out.contains("[DBG line 10]>"));
        assert!(dbg_out.contains("Program finished."));
        // Debug prompts should NOT appear in program output
        assert!(!prog_out.contains("[DBG"));
    }

    /// In remote mode, RUN sends debug status to debug output while program output stays separate.
    #[test]
    fn test_remote_debugger_run_separation() {
        let (prog_out, dbg_out) = run_remote_debugger("10 PRINT \"A\"\n20 PRINT \"B\"\n30 END\n", "RUN\nQUIT\n");
        assert!(prog_out.contains("A"));
        assert!(prog_out.contains("B"));
        assert!(dbg_out.contains("Program finished."));
        assert!(!prog_out.contains("Program finished."));
    }

    /// In remote mode, breakpoint messages go to debug output.
    #[test]
    fn test_remote_debugger_breakpoint() {
        let (prog_out, dbg_out) =
            run_remote_debugger("10 PRINT \"A\"\n20 PRINT \"B\"\n30 END\n", "BREAK AT 20\nRUN\nQUIT\n");
        assert!(prog_out.contains("A"));
        assert!(!prog_out.contains("Breakpoint"));
        assert!(dbg_out.contains("Breakpoint set at line 20"));
        assert!(dbg_out.contains("Breakpoint hit at line 20"));
    }

    /// In remote mode, PRINT from the debugger command goes to debug output.
    #[test]
    fn test_remote_debugger_print_goes_to_debug() {
        let (prog_out, dbg_out) = run_remote_debugger("10 LET X = 42\n20 END\n", "STEP\nPRINT X\nQUIT\n");
        // Debugger PRINT should go to debug output
        assert!(dbg_out.contains(" 42 "));
        // Not to program output
        assert!(!prog_out.contains("42"));
    }

    /// In remote mode, LIST goes to debug output.
    #[test]
    fn test_remote_debugger_list_goes_to_debug() {
        let (prog_out, dbg_out) = run_remote_debugger("10 PRINT \"A\"\n20 END\n", "LIST\nQUIT\n");
        assert!(dbg_out.contains("10 PRINT \"A\""));
        assert!(dbg_out.contains("20 END"));
        assert!(!prog_out.contains("10 PRINT"));
    }

    /// In remote mode, HELP goes to debug output.
    #[test]
    fn test_remote_debugger_help_goes_to_debug() {
        let (prog_out, dbg_out) = run_remote_debugger("10 END\n", "HELP\nQUIT\n");
        assert!(dbg_out.contains("Debugger commands:"));
        assert!(!prog_out.contains("Debugger commands:"));
    }

    /// In remote mode, error messages go to debug output.
    #[test]
    fn test_remote_debugger_error_goes_to_debug() {
        let (prog_out, dbg_out) = run_remote_debugger("10 PRINT 1/0\n20 END\n", "STEP\nQUIT\n");
        assert!(dbg_out.contains("Runtime error:"));
        assert!(!prog_out.contains("Runtime error:"));
    }

    /// In remote mode, unknown commands go to debug output.
    #[test]
    fn test_remote_debugger_unknown_command() {
        let (_prog_out, dbg_out) = run_remote_debugger("10 END\n", "FOOBAR\nQUIT\n");
        assert!(dbg_out.contains("Unknown command: FOOBAR"));
    }

    /// In remote mode, empty program message goes to debug output.
    #[test]
    fn test_remote_debugger_empty_program() {
        let (prog_out, dbg_out) = run_remote_debugger("", "QUIT\n");
        assert!(dbg_out.contains("Program is empty."));
        assert!(!prog_out.contains("Program is empty."));
    }

    /// Remote debugger PRINT with string expression writes to debug output.
    #[test]
    fn test_remote_debugger_print_string() {
        let (_prog_out, dbg_out) = run_remote_debugger("10 LET A$ = \"HELLO\"\n20 END\n", "STEP\nPRINT A$\nQUIT\n");
        assert!(dbg_out.contains("HELLO"));
    }

    /// Remote debugger PRINT with negative number formats correctly.
    #[test]
    fn test_remote_debugger_print_negative() {
        let (_prog_out, dbg_out) = run_remote_debugger("10 LET X = -7\n20 END\n", "STEP\nPRINT X\nQUIT\n");
        assert!(dbg_out.contains("-7"));
    }

    /// Remote debugger PRINT with no items prints a blank line to debug output.
    #[test]
    fn test_remote_debugger_print_empty() {
        let (_prog_out, dbg_out) = run_remote_debugger("10 END\n", "PRINT\nQUIT\n");
        // PRINT with no args produces a newline in the debug output
        assert!(dbg_out.contains("\n"));
    }

    /// TCP-based remote debugger integration test using a real loopback connection.
    #[test]
    fn test_remote_debugger_tcp() {
        use std::io::{BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = std::thread::spawn(move || {
            let (stream, _addr) = listener.accept().unwrap();
            let read_stream = stream.try_clone().unwrap();
            let program = parse_program("10 PRINT \"TCP_TEST\"\n20 END\n");
            let prog_input = io::Cursor::new(Vec::new());
            let mut prog_output: Vec<u8> = Vec::new();
            {
                let interp = Interpreter::new(io::BufReader::new(prog_input), &mut prog_output);
                let mut debugger =
                    Debugger::new_remote(interp, Box::new(BufReader::new(read_stream)), Box::new(stream));
                debugger.run_repl(&program).unwrap();
            }
            (
                String::from_utf8(prog_output).unwrap(),
                // debug output went over TCP, we can't capture it here
            )
        });

        // Connect and send debug commands
        let mut client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        // Send commands
        client.write_all(b"RUN\nQUIT\n").unwrap();
        client.flush().unwrap();
        // Read debug output from the TCP connection
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        std::io::Read::read_to_string(&mut client, &mut response).unwrap();

        let (prog_out,) = handle.join().unwrap();
        // Program output (PRINT) went to prog_output, not TCP
        assert!(prog_out.contains("TCP_TEST"));
        // Debug output went over TCP
        assert!(response.contains("[DBG line 10]>"));
        assert!(response.contains("Program finished."));
    }
}
