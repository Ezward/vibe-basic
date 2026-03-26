//! Program interpreter (runtime execution engine) for BASIC.
//!
//! This module provides the `Interpreter` struct which executes a parsed BASIC
//! `Program`. It manages control flow (sequential execution, GOTO, IF/THEN branching,
//! FOR/NEXT loops), performs I/O via generic `BufRead`/`Write` streams, and delegates
//! expression evaluation to the `Evaluator`. PRINT formatting follows MS-BASIC
//! conventions (14-character tab zones for commas, newline suppression with semicolons).

use crate::ast::{PrintItem, Program, Statement, ThenClause};
use crate::eval::{Evaluator, UserFunction, Value};
use std::io::{BufRead, Write};

/// Tracks the state of an active FOR loop on the loop stack.
#[derive(Debug, Clone)]
struct ForState {
    variable: String,
    end_val: f64,
    step_val: f64,
    /// Index into program lines where the FOR statement is
    line_index: usize,
    /// Index of the FOR statement within its line (for multi-statement lines)
    stmt_index: usize,
}

/// An entry in the collected DATA pool, recording its originating line number.
#[derive(Debug, Clone)]
struct DataEntry {
    value: Value,
    line_number: u32,
}

/// Runtime interpreter for BASIC programs, parameterized over input and output streams.
pub struct Interpreter<R: BufRead, W: Write> {
    pub(crate) evaluator: Evaluator,
    pub(crate) input: R,
    pub(crate) output: W,
    for_stack: Vec<ForState>,
    pub(crate) gosub_stack: Vec<GosubReturn>,
    column: usize,
    data_pool: Vec<DataEntry>,
    data_pointer: usize,
    /// Current text foreground color (0-31, default 7 = white/light gray)
    foreground_color: u8,
    /// Current text background color (0-7, default 0 = black)
    background_color: u8,
    /// Current border color (0-15, default 0 = black)
    border_color: u8,
    /// Whether screen positioning commands (LOCATE, CLS) have been used.
    /// When true, PRINT emits an ANSI clear-to-end-of-line escape before
    /// each newline so that repositioned text does not leave stale characters.
    screen_mode_active: bool,
}

/// Tracks the return address for a GOSUB call.
#[derive(Debug, Clone)]
pub(crate) struct GosubReturn {
    /// Index of the line to return to after RETURN
    pub(crate) line_index: usize,
    /// Index of the statement within that line to resume at
    pub(crate) stmt_index: usize,
}

impl<R: BufRead, W: Write> Interpreter<R, W> {
    /// Creates a new interpreter with the given input and output streams.
    pub fn new(input: R, output: W) -> Self {
        Interpreter {
            evaluator: Evaluator::new(),
            input,
            output,
            for_stack: Vec::new(),
            gosub_stack: Vec::new(),
            column: 0,
            data_pool: Vec::new(),
            data_pointer: 0,
            foreground_color: 7,
            background_color: 0,
            border_color: 0,
            screen_mode_active: false,
        }
    }

    /// Executes a BASIC program from the first line to completion. Handles sequential
    /// execution, GOTO jumps, IF/THEN branching, FOR/NEXT loops, and END termination.
    /// Errors include source line context for debugging.
    pub fn run(&mut self, program: &Program) -> Result<(), String> {
        if program.lines.is_empty() {
            return Ok(());
        }

        self.collect_data(program)?;
        let mut line_idx = 0;
        let mut start_stmt_idx = 0;

        while line_idx < program.lines.len() {
            let line = &program.lines[line_idx];
            let mut stmt_idx = start_stmt_idx;
            start_stmt_idx = 0;
            let mut next_line_idx = line_idx + 1;

            while stmt_idx < line.statements.len() {
                let stmt = &line.statements[stmt_idx];
                let result = self.execute_statement(stmt, line_idx, stmt_idx, program).map_err(|e| {
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
                    StmtResult::Gosub(target_line) => {
                        // Push return address: next statement on this line, or next line
                        let return_addr = if stmt_idx + 1 < line.statements.len() {
                            GosubReturn {
                                line_index: line_idx,
                                stmt_index: stmt_idx + 1,
                            }
                        } else {
                            GosubReturn {
                                line_index: line_idx + 1,
                                stmt_index: 0,
                            }
                        };
                        self.gosub_stack.push(return_addr);
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
                    StmtResult::Return(target_line) => {
                        // Always pop the gosub stack
                        let _ret = self.gosub_stack.pop().unwrap();
                        if let Some(line_num) = target_line {
                            // RETURN with line number: jump to specified line instead of return address
                            next_line_idx = self.find_line_index(program, line_num).map_err(|e| {
                                let source_text = program
                                    .source_lines
                                    .get(line.source_line - 1)
                                    .map(|s| s.as_str())
                                    .unwrap_or("<unknown>");
                                format!("{}\n  at line {}: {}", e, line.source_line, source_text)
                            })?;
                        } else {
                            // RETURN without line number: use stacked return address
                            next_line_idx = _ret.line_index;
                            start_stmt_idx = _ret.stmt_index;
                        }
                        break;
                    }
                    StmtResult::End => return Ok(()),
                    StmtResult::SkipLine => {
                        // IF condition was false - skip remaining statements on this line
                        break;
                    }
                    StmtResult::ForLoopSkip { line_index, stmt_index } => {
                        next_line_idx = line_index;
                        start_stmt_idx = stmt_index;
                        break;
                    }
                    StmtResult::ForLoopBack { line_index, stmt_index } => {
                        next_line_idx = line_index;
                        start_stmt_idx = stmt_index;
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
        current_stmt_idx: usize,
        program: &Program,
    ) -> Result<StmtResult, String> {
        match stmt {
            Statement::Let {
                variable,
                indices,
                expression,
            } => {
                let value = self.evaluator.eval_expr(expression)?;
                if indices.is_empty() {
                    self.evaluator.variables.insert(variable.clone(), value);
                } else {
                    let subs = self.evaluator.eval_subscripts(indices)?;
                    self.evaluator.set_array_element(variable, &subs, value)?;
                }
                Ok(StmtResult::Continue)
            }
            Statement::Print { items } => {
                self.execute_print(items)?;
                Ok(StmtResult::Continue)
            }
            Statement::If {
                condition,
                then,
                else_clause,
            } => {
                let val = self.evaluator.eval_expr(condition)?;
                if val.is_truthy() {
                    match then.as_ref() {
                        ThenClause::LineNumber(n) => Ok(StmtResult::Goto(*n)),
                        ThenClause::Statement(inner_stmt) => {
                            self.execute_statement(inner_stmt, current_line_idx, current_stmt_idx, program)
                        }
                    }
                } else if let Some(else_cl) = else_clause {
                    match else_cl.as_ref() {
                        ThenClause::LineNumber(n) => Ok(StmtResult::Goto(*n)),
                        ThenClause::Statement(inner_stmt) => {
                            self.execute_statement(inner_stmt, current_line_idx, current_stmt_idx, program)
                        }
                    }
                } else {
                    Ok(StmtResult::SkipLine)
                }
            }
            Statement::Goto { line_number } => Ok(StmtResult::Goto(*line_number)),
            Statement::Input {
                prompt,
                variable,
                indices,
                suppress_question_mark,
            } => {
                self.execute_input(prompt.as_deref(), variable, indices, *suppress_question_mark)?;
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
                    // Skip to the statement after the matching NEXT
                    let (next_line_idx, next_stmt_idx) =
                        self.find_matching_next(program, current_line_idx, current_stmt_idx, variable)?;
                    // Continue after the NEXT: if there are more statements on the same line,
                    // resume at the next statement; otherwise go to the next line.
                    if next_stmt_idx + 1 < program.lines[next_line_idx].statements.len() {
                        return Ok(StmtResult::ForLoopSkip {
                            line_index: next_line_idx,
                            stmt_index: next_stmt_idx + 1,
                        });
                    } else {
                        return Ok(StmtResult::ForLoopSkip {
                            line_index: next_line_idx + 1,
                            stmt_index: 0,
                        });
                    }
                }

                self.for_stack.push(ForState {
                    variable: variable.clone(),
                    end_val,
                    step_val,
                    line_index: current_line_idx,
                    stmt_index: current_stmt_idx,
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
                    // Jump back to the statement after the FOR statement
                    Ok(StmtResult::ForLoopBack {
                        line_index: for_state.line_index,
                        stmt_index: for_state.stmt_index + 1,
                    })
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
            Statement::DefFn { name, params, body } => {
                self.evaluator.user_functions.insert(
                    name.clone(),
                    UserFunction {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
                Ok(StmtResult::Continue)
            }
            Statement::Data { .. } => {
                // DATA statements are non-executable; values are collected at program start.
                Ok(StmtResult::Continue)
            }
            Statement::Read { variables } => {
                for (var_name, indices) in variables {
                    if self.data_pointer >= self.data_pool.len() {
                        return Err("Out of DATA".to_string());
                    }
                    let entry = self.data_pool[self.data_pointer].clone();
                    self.data_pointer += 1;
                    // Assign value, converting types as needed
                    let value = if var_name.ends_with('$') {
                        // String variable expects a string value
                        match entry.value {
                            Value::String(_) => entry.value,
                            Value::Number(n) => Value::String(format!("{}", Value::Number(n))),
                        }
                    } else {
                        // Numeric variable expects a number
                        match &entry.value {
                            Value::Number(_) => entry.value,
                            Value::String(s) => {
                                if let Ok(n) = s.parse::<f64>() {
                                    Value::Number(n)
                                } else {
                                    return Err(format!("Type mismatch: READ expected number, got \"{}\"", s));
                                }
                            }
                        }
                    };
                    if indices.is_empty() {
                        self.evaluator.variables.insert(var_name.clone(), value);
                    } else {
                        let subs = self.evaluator.eval_subscripts(indices)?;
                        self.evaluator.set_array_element(var_name, &subs, value)?;
                    }
                }
                Ok(StmtResult::Continue)
            }
            Statement::Restore { line_number } => {
                if let Some(target) = line_number {
                    // Find the first data entry from the specified line number
                    if let Some(idx) = self.data_pool.iter().position(|e| e.line_number >= *target) {
                        self.data_pointer = idx;
                    } else {
                        // No DATA at or after that line; reset to end (next READ will fail)
                        self.data_pointer = self.data_pool.len();
                    }
                } else {
                    self.data_pointer = 0;
                }
                Ok(StmtResult::Continue)
            }
            Statement::Dim { arrays } => {
                for (name, dim_exprs) in arrays {
                    if self.evaluator.arrays.contains_key(name) {
                        return Err(format!("Array {} already dimensioned", name));
                    }
                    let mut dims = Vec::new();
                    for expr in dim_exprs {
                        let max_sub = self.evaluator.eval_expr(expr)?.as_number()? as i64;
                        if max_sub < 0 {
                            return Err(format!("Invalid dimension: {}", max_sub));
                        }
                        dims.push((max_sub + 1) as usize); // subscripts 0..=max_sub
                    }
                    let is_string = name.ends_with('$');
                    self.evaluator
                        .arrays
                        .insert(name.clone(), crate::eval::Array::new(dims, is_string));
                }
                Ok(StmtResult::Continue)
            }
            Statement::Erase { arrays } => {
                for name in arrays {
                    self.evaluator.arrays.remove(name);
                }
                Ok(StmtResult::Continue)
            }
            Statement::Gosub { target } => {
                let val = self.evaluator.eval_expr(target)?;
                let line_num = val.as_number()? as u32;
                Ok(StmtResult::Gosub(line_num))
            }
            Statement::OnGosub { selector, targets } => {
                let val = self.evaluator.eval_expr(selector)?;
                let index = val.as_number()? as i64;
                if index < 1 || index as usize > targets.len() {
                    // Out of range: continue to next statement (GW-BASIC behavior)
                    Ok(StmtResult::Continue)
                } else {
                    let line_num = targets[(index - 1) as usize];
                    Ok(StmtResult::Gosub(line_num))
                }
            }
            Statement::Return { target } => {
                if self.gosub_stack.is_empty() {
                    return Err("RETURN without GOSUB".to_string());
                }
                let target_line = match target {
                    Some(expr) => {
                        let val = self.evaluator.eval_expr(expr)?;
                        Some(val.as_number()? as u32)
                    }
                    None => None,
                };
                Ok(StmtResult::Return(target_line))
            }
            Statement::Locate {
                row,
                col,
                cursor,
                start,
                stop,
            } => {
                self.execute_locate(row, col, cursor, start, stop)?;
                Ok(StmtResult::Continue)
            }
            Statement::Cls { mode } => {
                self.execute_cls(mode)?;
                Ok(StmtResult::Continue)
            }
            Statement::Color {
                foreground,
                background,
                border,
            } => {
                self.execute_color(foreground, background, border)?;
                Ok(StmtResult::Continue)
            }
            Statement::Rem(_) => Ok(StmtResult::Continue),
            Statement::End => Ok(StmtResult::End),
        }
    }

    /// Scans all DATA statements in program-line-number order and collects their
    /// constant values into a flat pool for READ to consume.
    pub(crate) fn collect_data(&mut self, program: &Program) -> Result<(), String> {
        self.data_pool.clear();
        self.data_pointer = 0;
        for line in &program.lines {
            for stmt in &line.statements {
                if let Statement::Data { values } = stmt {
                    for expr in values {
                        let value = self.evaluator.eval_expr(expr)?;
                        self.data_pool.push(DataEntry {
                            value,
                            line_number: line.line_number,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes a PRINT statement. Commas advance to the next 14-character tab zone,
    /// semicolons suppress spacing, and a trailing separator suppresses the newline.
    pub(crate) fn execute_print(&mut self, items: &[PrintItem]) -> Result<(), String> {
        if items.is_empty() {
            self.write_eol()?;
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
            self.write_eol()?;
            self.column = 0;
        }

        Ok(())
    }

    /// Writes an end-of-line sequence. When screen positioning commands have been
    /// used, emits an ANSI clear-to-end-of-line escape (`\x1b[K`) before the newline
    /// so that repositioned text does not leave stale characters from previous output.
    fn write_eol(&mut self) -> Result<(), String> {
        if self.screen_mode_active {
            writeln!(self.output, "\x1b[K").map_err(|e| e.to_string())
        } else {
            writeln!(self.output).map_err(|e| e.to_string())
        }
    }

    /// Executes an INPUT statement: prints an optional prompt, reads a line,
    /// and stores it as a string (for $ variables) or parses it as a number.
    /// Supports both scalar variables and array elements.
    ///
    /// When `suppress_question_mark` is false (semicolon separator), a "? " is appended
    /// after any prompt. When true (comma separator), the question mark is suppressed.
    fn execute_input(
        &mut self,
        prompt: Option<&str>,
        variable: &str,
        indices: &[crate::expr::Expr],
        suppress_question_mark: bool,
    ) -> Result<(), String> {
        if let Some(p) = prompt {
            write!(self.output, "{}", p).map_err(|e| e.to_string())?;
        }
        if !suppress_question_mark {
            write!(self.output, "? ").map_err(|e| e.to_string())?;
        }
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

        if indices.is_empty() {
            self.evaluator.variables.insert(variable.to_string(), value);
        } else {
            let subs = self.evaluator.eval_subscripts(indices)?;
            self.evaluator.set_array_element(variable, &subs, value)?;
        }
        self.column = 0;
        Ok(())
    }

    /// Maps a GW-BASIC color number (0-15) to the corresponding ANSI SGR color code.
    /// Colors 0-7 map to ANSI codes 30-37 (normal), colors 8-15 map to 90-97 (bright).
    fn basic_fg_to_ansi(color: u8) -> &'static str {
        match color & 0x0F {
            0 => "30",  // Black
            1 => "34",  // Blue
            2 => "32",  // Green
            3 => "36",  // Cyan
            4 => "31",  // Red
            5 => "35",  // Magenta
            6 => "33",  // Brown/Yellow
            7 => "37",  // White/Light gray
            8 => "90",  // Dark gray (bright black)
            9 => "94",  // Light blue
            10 => "92", // Light green
            11 => "96", // Light cyan
            12 => "91", // Light red
            13 => "95", // Light magenta
            14 => "93", // Yellow
            15 => "97", // Bright white
            _ => "37",  // Fallback
        }
    }

    /// Maps a GW-BASIC background color number (0-7) to the corresponding ANSI SGR background code.
    fn basic_bg_to_ansi(color: u8) -> &'static str {
        match color & 0x07 {
            0 => "40", // Black
            1 => "44", // Blue
            2 => "42", // Green
            3 => "46", // Cyan
            4 => "41", // Red
            5 => "45", // Magenta
            6 => "43", // Brown/Yellow
            7 => "47", // White
            _ => "40", // Fallback
        }
    }

    /// Emits ANSI escape codes to set the current foreground and background colors.
    fn emit_color_ansi(&mut self) -> Result<(), String> {
        let blink = if self.foreground_color >= 16 { ";5" } else { "" };
        let fg = Self::basic_fg_to_ansi(self.foreground_color);
        let bg = Self::basic_bg_to_ansi(self.background_color);
        write!(self.output, "\x1b[{}{};{}m", fg, blink, bg).map_err(|e| e.to_string())
    }

    /// Executes a LOCATE statement using ANSI escape codes for cursor positioning.
    /// LOCATE [row][,[col][,[cursor][,[start][,stop]]]]
    fn execute_locate(
        &mut self,
        row: &Option<crate::expr::Expr>,
        col: &Option<crate::expr::Expr>,
        cursor: &Option<crate::expr::Expr>,
        start: &Option<crate::expr::Expr>,
        stop: &Option<crate::expr::Expr>,
    ) -> Result<(), String> {
        self.screen_mode_active = true;
        let row_val = match row {
            Some(expr) => {
                let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
                if !(1..=25).contains(&v) {
                    return Err("Illegal function call".to_string());
                }
                Some(v)
            }
            None => None,
        };
        let col_val = match col {
            Some(expr) => {
                let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
                if !(1..=80).contains(&v) {
                    return Err("Illegal function call".to_string());
                }
                Some(v)
            }
            None => None,
        };
        let cursor_val = match cursor {
            Some(expr) => Some(self.evaluator.eval_expr(expr)?.as_number()? as i32),
            None => None,
        };
        let _start_val = match start {
            Some(expr) => {
                let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
                if !(0..=31).contains(&v) {
                    return Err("Illegal function call".to_string());
                }
                Some(v)
            }
            None => None,
        };
        let _stop_val = match stop {
            Some(expr) => {
                let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
                if !(0..=31).contains(&v) {
                    return Err("Illegal function call".to_string());
                }
                Some(v)
            }
            None => None,
        };

        // Emit ANSI cursor position escape sequence
        match (row_val, col_val) {
            (Some(r), Some(c)) => {
                write!(self.output, "\x1b[{};{}H", r, c).map_err(|e| e.to_string())?;
                self.column = (c - 1) as usize;
            }
            (Some(r), None) => {
                // Move to row, keep column (move to row, column 1 as default if unknown)
                write!(self.output, "\x1b[{};1H", r).map_err(|e| e.to_string())?;
                self.column = 0;
            }
            (None, Some(c)) => {
                // Move to column only using cursor horizontal absolute
                write!(self.output, "\x1b[{}G", c).map_err(|e| e.to_string())?;
                self.column = (c - 1) as usize;
            }
            (None, None) => {
                // No position change
            }
        }

        // Handle cursor visibility
        if let Some(v) = cursor_val {
            if v == 0 {
                write!(self.output, "\x1b[?25l").map_err(|e| e.to_string())?;
            } else {
                write!(self.output, "\x1b[?25h").map_err(|e| e.to_string())?;
            }
        }

        // Note: start/stop scan lines for cursor shape are validated but not emitted
        // since ANSI terminals don't support hardware cursor raster line control.

        Ok(())
    }

    /// Executes a CLS statement using ANSI escape codes to clear the screen.
    /// CLS [n] — in text mode: 0 or no argument clears entire screen, 2 clears text window.
    fn execute_cls(&mut self, mode: &Option<crate::expr::Expr>) -> Result<(), String> {
        self.screen_mode_active = true;
        let mode_val = match mode {
            Some(expr) => self.evaluator.eval_expr(expr)?.as_number()? as i32,
            None => 0,
        };
        match mode_val {
            0 | 2 => {
                // Clear entire screen and move cursor to upper-left corner
                write!(self.output, "\x1b[2J\x1b[H").map_err(|e| e.to_string())?;
                self.column = 0;
            }
            1 => {
                // In text mode, CLS 1 (graphics viewport) is a no-op since we have no graphics
                // Clear screen as a reasonable fallback
                write!(self.output, "\x1b[2J\x1b[H").map_err(|e| e.to_string())?;
                self.column = 0;
            }
            _ => {
                return Err("Illegal function call".to_string());
            }
        }
        Ok(())
    }

    /// Executes a COLOR statement using ANSI escape codes for text coloring.
    /// COLOR [foreground][,[background][,border]]
    /// Foreground: 0-31 (0-15 normal, 16-31 blinking). Background: 0-7. Border: 0-15.
    fn execute_color(
        &mut self,
        foreground: &Option<crate::expr::Expr>,
        background: &Option<crate::expr::Expr>,
        border: &Option<crate::expr::Expr>,
    ) -> Result<(), String> {
        if let Some(expr) = foreground {
            let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
            if !(0..=31).contains(&v) {
                return Err("Illegal function call".to_string());
            }
            self.foreground_color = v as u8;
        }
        if let Some(expr) = background {
            let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
            if !(0..=7).contains(&v) {
                return Err("Illegal function call".to_string());
            }
            self.background_color = v as u8;
        }
        if let Some(expr) = border {
            let v = self.evaluator.eval_expr(expr)?.as_number()? as i32;
            if !(0..=15).contains(&v) {
                return Err("Illegal function call".to_string());
            }
            self.border_color = v as u8;
        }
        // Emit ANSI color codes
        self.emit_color_ansi()?;
        Ok(())
    }

    /// Searches forward from a FOR statement to find its matching NEXT, respecting
    /// nesting depth of intervening FOR/NEXT pairs. Searches remaining statements
    /// on the current line first, then subsequent lines.
    /// Returns (line_index, stmt_index) of the matching NEXT.
    fn find_matching_next(
        &self,
        program: &Program,
        for_line_idx: usize,
        for_stmt_idx: usize,
        var: &str,
    ) -> Result<(usize, usize), String> {
        let mut depth = 0;
        // Search remaining statements on the same line (after the FOR)
        let line = &program.lines[for_line_idx];
        for (offset, stmt) in line.statements[(for_stmt_idx + 1)..].iter().enumerate() {
            let s_idx = for_stmt_idx + 1 + offset;
            match stmt {
                Statement::For { .. } => depth += 1,
                Statement::Next { variable } => {
                    if depth == 0 {
                        if let Some(v) = variable {
                            if v == var {
                                return Ok((for_line_idx, s_idx));
                            }
                        } else {
                            return Ok((for_line_idx, s_idx));
                        }
                    } else {
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }
        // Search subsequent lines
        for i in (for_line_idx + 1)..program.lines.len() {
            for (s_idx, stmt) in program.lines[i].statements.iter().enumerate() {
                match stmt {
                    Statement::For { .. } => depth += 1,
                    Statement::Next { variable } => {
                        if depth == 0 {
                            if let Some(v) = variable {
                                if v == var {
                                    return Ok((i, s_idx));
                                }
                            } else {
                                return Ok((i, s_idx));
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
    Gosub(u32),
    Return(Option<u32>),
    End,
    SkipLine,
    /// Skip past a FOR loop body entirely (initial value already exceeds limit).
    ForLoopSkip {
        line_index: usize,
        stmt_index: usize,
    },
    /// NEXT loops back to the statement after the FOR on the same (or different) line.
    ForLoopBack {
        line_index: usize,
        stmt_index: usize,
    },
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
    fn test_input_with_prompt_comma_suppresses_question_mark() {
        let output = run_program_with_input("10 INPUT \"NAME: \", N$\n20 PRINT N$\n30 END\n", "BOB\n").unwrap();
        assert_eq!(output, "NAME: BOB\n");
    }

    #[test]
    fn test_input_with_prompt_semicolon_shows_question_mark() {
        let output = run_program_with_input("10 INPUT \"NAME: \"; N$\n20 PRINT N$\n30 END\n", "BOB\n").unwrap();
        assert_eq!(output, "NAME: ? BOB\n");
    }

    #[test]
    fn test_input_comma_numeric() {
        let output = run_program_with_input("10 INPUT \"VALUE\", G\n20 PRINT G + 1\n30 END\n", "7\n").unwrap();
        assert_eq!(output, "VALUE 8 \n");
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
        let output =
            run_program("10 LET X = 1\n20 IF X = 1 THEN 40\n30 PRINT \"BAD\"\n40 PRINT \"GOOD\"\n50 END\n").unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_if_false_skips_remaining_statements_on_line() {
        // IF false should skip remaining statements on the same line (after colon)
        let output = run_program("10 LET X = 0\n20 IF X = 1 THEN PRINT \"YES\"\n30 PRINT \"DONE\"\n40 END\n").unwrap();
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
        let output = run_program_with_input("10 INPUT G\n20 END\n", "HELLO\n").unwrap();
        assert_eq!(output, "? ");
    }

    #[test]
    fn test_for_loop_negative_step_skip() {
        // Negative step where start > end should skip
        let output =
            run_program("10 FOR I = 1 TO 10 STEP -1\n20 PRINT \"INSIDE\"\n30 NEXT I\n40 PRINT \"DONE\"\n50 END\n")
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
        let result = run_program("10 PRINT 1/0\n20 END\n");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("at line"));
    }

    #[test]
    fn test_undefined_variable_auto_initializes() {
        let output = run_program("10 PRINT X\n20 PRINT X$\n30 END\n").unwrap();
        assert_eq!(output, " 0 \n\n");
    }

    #[test]
    fn test_if_else_line_number_falsy() {
        let output =
            run_program("10 LET X = 0\n20 IF X = 1 THEN 50 ELSE 40\n30 PRINT \"BAD\"\n40 PRINT \"GOOD\"\n50 END\n")
                .unwrap();
        assert_eq!(output, "GOOD\n");
    }

    #[test]
    fn test_for_skip_with_next_no_variable() {
        // FOR loop skipped, and matching NEXT has no variable
        let output =
            run_program("10 FOR I = 10 TO 1\n20 PRINT \"INSIDE\"\n30 NEXT\n40 PRINT \"DONE\"\n50 END\n").unwrap();
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

    #[test]
    fn test_if_with_and() {
        let output = run_program(
            "\
10 LET X = 5
20 LET Y = 3
30 IF X > 0 AND Y > 0 THEN PRINT \"BOTH POSITIVE\"
40 END
",
        )
        .unwrap();
        assert_eq!(output, "BOTH POSITIVE\n");
    }

    #[test]
    fn test_if_with_and_false() {
        let output = run_program(
            "\
10 LET X = 5
20 LET Y = -1
30 IF X > 0 AND Y > 0 THEN PRINT \"BOTH POSITIVE\"
40 PRINT \"DONE\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_if_with_or() {
        let output = run_program(
            "\
10 LET X = -1
20 LET Y = 5
30 IF X > 0 OR Y > 0 THEN PRINT \"AT LEAST ONE POSITIVE\"
40 END
",
        )
        .unwrap();
        assert_eq!(output, "AT LEAST ONE POSITIVE\n");
    }

    #[test]
    fn test_if_with_or_both_false() {
        let output = run_program(
            "\
10 LET X = -1
20 LET Y = -2
30 IF X > 0 OR Y > 0 THEN PRINT \"POSITIVE\"
40 PRINT \"DONE\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_if_with_not() {
        let output = run_program(
            "\
10 LET X = 0
20 IF NOT X = 1 THEN PRINT \"NOT ONE\"
30 END
",
        )
        .unwrap();
        assert_eq!(output, "NOT ONE\n");
    }

    #[test]
    fn test_if_with_xor() {
        let output = run_program(
            "\
10 LET A = 1
20 LET B = 0
30 IF (A = 1) XOR (B = 1) THEN PRINT \"EXACTLY ONE\"
40 END
",
        )
        .unwrap();
        assert_eq!(output, "EXACTLY ONE\n");
    }

    #[test]
    fn test_if_with_xor_both_true() {
        let output = run_program(
            "\
10 LET A = 1
20 LET B = 1
30 IF (A = 1) XOR (B = 1) THEN PRINT \"EXACTLY ONE\"
40 PRINT \"DONE\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "DONE\n");
    }

    #[test]
    fn test_complex_logical_in_loop() {
        let output = run_program(
            "\
10 FOR I = 1 TO 10
20 IF I > 3 AND I < 7 THEN PRINT I;
30 NEXT I
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 4  5  6 ");
    }

    #[test]
    fn test_not_in_if_else() {
        let output = run_program(
            "\
10 LET X = 5
20 IF NOT X = 5 THEN PRINT \"NOT FIVE\" ELSE PRINT \"FIVE\"
30 END
",
        )
        .unwrap();
        assert_eq!(output, "FIVE\n");
    }

    // --- String function integration tests ---

    #[test]
    fn test_left_function() {
        let output = run_program("10 PRINT LEFT$(\"HELLO WORLD\", 5)\n20 END\n").unwrap();
        assert_eq!(output, "HELLO\n");
    }

    #[test]
    fn test_right_function() {
        let output = run_program("10 PRINT RIGHT$(\"HELLO WORLD\", 5)\n20 END\n").unwrap();
        assert_eq!(output, "WORLD\n");
    }

    #[test]
    fn test_mid_function() {
        let output = run_program("10 PRINT MID$(\"HELLO WORLD\", 7, 5)\n20 END\n").unwrap();
        assert_eq!(output, "WORLD\n");
    }

    #[test]
    fn test_mid_function_no_length() {
        let output = run_program("10 PRINT MID$(\"HELLO WORLD\", 7)\n20 END\n").unwrap();
        assert_eq!(output, "WORLD\n");
    }

    #[test]
    fn test_instr_function() {
        let output = run_program("10 PRINT INSTR(\"HELLO WORLD\", \"WORLD\")\n20 END\n").unwrap();
        assert_eq!(output, " 7 \n");
    }

    #[test]
    fn test_asc_function() {
        let output = run_program("10 PRINT ASC(\"A\")\n20 END\n").unwrap();
        assert_eq!(output, " 65 \n");
    }

    #[test]
    fn test_chr_function() {
        let output = run_program("10 PRINT CHR$(65)\n20 END\n").unwrap();
        assert_eq!(output, "A\n");
    }

    #[test]
    fn test_str_function() {
        let output = run_program("10 PRINT \"NUM:\" + STR$(42)\n20 END\n").unwrap();
        assert_eq!(output, "NUM: 42\n");
    }

    #[test]
    fn test_val_function() {
        let output = run_program("10 PRINT VAL(\"42\") + 8\n20 END\n").unwrap();
        assert_eq!(output, " 50 \n");
    }

    #[test]
    fn test_hex_function() {
        let output = run_program("10 PRINT HEX$(255)\n20 END\n").unwrap();
        assert_eq!(output, "FF\n");
    }

    #[test]
    fn test_oct_function() {
        let output = run_program("10 PRINT OCT$(8)\n20 END\n").unwrap();
        assert_eq!(output, "10\n");
    }

    #[test]
    fn test_string_function() {
        let output = run_program("10 PRINT STRING$(5, \"*\")\n20 END\n").unwrap();
        assert_eq!(output, "*****\n");
    }

    #[test]
    fn test_space_function() {
        let output = run_program("10 PRINT \"A\"; SPACE$(3); \"B\"\n20 END\n").unwrap();
        assert_eq!(output, "A   B\n");
    }

    #[test]
    fn test_spc_function() {
        let output = run_program("10 PRINT \"A\"; SPC(5); \"B\"\n20 END\n").unwrap();
        assert_eq!(output, "A     B\n");
    }

    #[test]
    fn test_string_functions_in_loop() {
        let output = run_program(
            "\
10 LET S$ = \"ABCDE\"
20 FOR I = 1 TO 5
30 PRINT MID$(S$, I, 1);
40 NEXT I
50 END
",
        )
        .unwrap();
        assert_eq!(output, "ABCDE");
    }

    #[test]
    fn test_string_reverse_with_functions() {
        let output = run_program(
            "\
10 LET S$ = \"ABCD\"
20 LET R$ = \"\"
30 FOR I = LEN(S$) TO 1 STEP -1
40 R$ = R$ + MID$(S$, I, 1)
50 NEXT I
60 PRINT R$
70 END
",
        )
        .unwrap();
        assert_eq!(output, "DCBA\n");
    }

    #[test]
    fn test_val_str_roundtrip_program() {
        let output = run_program(
            "\
10 LET X = 123
20 LET S$ = STR$(X)
30 LET Y = VAL(S$)
40 IF Y = X THEN PRINT \"MATCH\"
50 END
",
        )
        .unwrap();
        assert_eq!(output, "MATCH\n");
    }

    #[test]
    fn test_hex_oct_conversion() {
        let output = run_program(
            "\
10 PRINT \"HEX:\"; HEX$(42)
20 PRINT \"OCT:\"; OCT$(42)
30 END
",
        )
        .unwrap();
        assert_eq!(output, "HEX:2A\nOCT:52\n");
    }

    // --- Math function integration tests ---

    #[test]
    fn test_exp_function() {
        let output = run_program("10 PRINT EXP(0)\n20 END\n").unwrap();
        assert_eq!(output, " 1 \n");
    }

    #[test]
    fn test_log_function() {
        let output = run_program("10 PRINT LOG(1)\n20 END\n").unwrap();
        assert_eq!(output, " 0 \n");
    }

    #[test]
    fn test_sgn_function() {
        let output = run_program(
            "\
10 PRINT SGN(5); SGN(-3); SGN(0)
20 END
",
        )
        .unwrap();
        assert_eq!(output, " 1 -1  0 \n");
    }

    #[test]
    fn test_sin_cos_function() {
        let output = run_program("10 PRINT SIN(0); COS(0)\n20 END\n").unwrap();
        assert_eq!(output, " 0  1 \n");
    }

    #[test]
    fn test_tan_function() {
        let output = run_program("10 PRINT TAN(0)\n20 END\n").unwrap();
        assert_eq!(output, " 0 \n");
    }

    #[test]
    fn test_atn_function() {
        let output = run_program("10 PRINT ATN(0)\n20 END\n").unwrap();
        assert_eq!(output, " 0 \n");
    }

    #[test]
    fn test_fix_function() {
        let output = run_program("10 PRINT FIX(3.7); FIX(-3.7)\n20 END\n").unwrap();
        assert_eq!(output, " 3 -3 \n");
    }

    #[test]
    fn test_cint_function() {
        let output = run_program("10 PRINT CINT(3.6); CINT(3.2)\n20 END\n").unwrap();
        assert_eq!(output, " 4  3 \n");
    }

    #[test]
    fn test_csng_function() {
        let output = run_program("10 PRINT CSNG(42)\n20 END\n").unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_cdbl_function() {
        let output = run_program("10 PRINT CDBL(42)\n20 END\n").unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_int_vs_fix_negative() {
        // INT rounds toward -infinity, FIX truncates toward zero
        let output = run_program("10 PRINT INT(-3.7); FIX(-3.7)\n20 END\n").unwrap();
        assert_eq!(output, "-4 -3 \n");
    }

    // --- DEF FN integration tests ---

    #[test]
    fn test_def_fn_simple() {
        let output = run_program(
            "\
10 DEF FNMUL(A, B) = A * B
20 PRINT FNMUL(10, 5)
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 50 \n");
    }

    #[test]
    fn test_def_fn_no_params() {
        let output = run_program(
            "\
10 DEF FNPI = 3.14159
20 PRINT FNPI
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 3.14159 \n");
    }

    #[test]
    fn test_def_fn_with_global_var() {
        let output = run_program(
            "\
10 DEF FNADD(X) = X + BASE
20 BASE = 100
30 PRINT FNADD(5)
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 105 \n");
    }

    #[test]
    fn test_def_fn_params_are_local() {
        let output = run_program(
            "\
10 DEF FNSQR(X) = X * X
20 X = 99
30 PRINT FNSQR(5)
40 PRINT X
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 25 \n 99 \n");
    }

    #[test]
    fn test_def_fn_with_space() {
        // DEF FN MUL(A, B) with space between FN and name
        let output = run_program(
            "\
10 DEF FN MUL(A, B) = A * B
20 PRINT FN MUL(3, 4)
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 12 \n");
    }

    #[test]
    fn test_def_fn_in_expression() {
        let output = run_program(
            "\
10 DEF FNDBL(X) = X * 2
20 PRINT FNDBL(3) + FNDBL(4)
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 14 \n");
    }

    #[test]
    fn test_def_fn_string() {
        let output = run_program(
            "\
10 DEF FNGET$(X$) = LEFT$(X$, 1)
20 PRINT FNGET$(\"HELLO\")
30 END
",
        )
        .unwrap();
        assert_eq!(output, "H\n");
    }

    #[test]
    fn test_def_fn_wrong_arg_count() {
        let result = run_program(
            "\
10 DEF FNADD(A, B) = A + B
20 PRINT FNADD(1)
30 END
",
        );
        assert!(result.is_err());
    }

    // --- Trig identity test ---

    #[test]
    fn test_trig_identity_sin_sq_cos_sq() {
        // SIN(x)^2 + COS(x)^2 = 1 for any x
        let output = run_program(
            "\
10 X = 1.234
20 S = SIN(X) ^ 2 + COS(X) ^ 2
30 PRINT CINT(S)
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 1 \n");
    }

    // --- Derived formula: inverse sine ---

    #[test]
    fn test_derived_inverse_sine() {
        // Inverse Sine using ATN: ATN(X / SQR(-X * X + 1))
        let output = run_program(
            "\
10 X = 0.5
20 ASINE = ATN(X / SQR(-X * X + 1))
30 PRINT CINT(SIN(ASINE) * 1000)
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 500 \n");
    }

    #[test]
    fn test_def_fn_bare_fn_no_name() {
        // DEF FN = expr (bare FN with no name, parameterless)
        let output = run_program(
            "\
10 DEF FN = 42
20 PRINT FN
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_def_fn_parameterless_called_as_variable_with_params_error() {
        // Define a function with params, then reference it without parens
        let result = run_program(
            "\
10 DEF FNADD(A, B) = A + B
20 X = FNADD
30 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expects 2 argument(s)"));
    }

    // --- DATA / READ / RESTORE tests ---

    #[test]
    fn test_data_read_basic() {
        let output = run_program(
            "\
10 DATA 10, 20, 30
20 READ A, B, C
30 PRINT A; B; C
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 10  20  30 \n");
    }

    #[test]
    fn test_data_read_strings() {
        let output = run_program(
            "\
10 DATA \"HELLO\", \"WORLD\"
20 READ A$, B$
30 PRINT A$; \" \"; B$
40 END
",
        )
        .unwrap();
        assert_eq!(output, "HELLO WORLD\n");
    }

    #[test]
    fn test_data_read_mixed_types() {
        let output = run_program(
            "\
10 DATA \"ALICE\", 25, \"BOB\", 30
20 READ N1$, A1, N2$, A2
30 PRINT N1$; A1
40 PRINT N2$; A2
50 END
",
        )
        .unwrap();
        assert_eq!(output, "ALICE 25 \nBOB 30 \n");
    }

    #[test]
    fn test_data_across_multiple_lines() {
        let output = run_program(
            "\
10 DATA 1, 2, 3
20 DATA 4, 5, 6
30 READ A, B, C, D, E, F
40 PRINT A; B; C; D; E; F
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3  4  5  6 \n");
    }

    #[test]
    fn test_data_read_in_loop() {
        let output = run_program(
            "\
10 DATA 3.08, 5.19, 3.12, 3.98, 4.24
20 FOR I = 1 TO 5
30 READ A
40 PRINT A;
50 NEXT I
60 END
",
        )
        .unwrap();
        assert_eq!(output, " 3.08  5.19  3.12  3.98  4.24 ");
    }

    #[test]
    fn test_data_out_of_data_error() {
        let result = run_program(
            "\
10 DATA 1, 2
20 READ A, B, C
30 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Out of DATA"));
    }

    #[test]
    fn test_restore_resets_pointer() {
        let output = run_program(
            "\
10 DATA 57, 68, 79
20 READ A, B, C
30 RESTORE
40 READ D, E, F
50 PRINT A; B; C
60 PRINT D; E; F
70 END
",
        )
        .unwrap();
        assert_eq!(output, " 57  68  79 \n 57  68  79 \n");
    }

    #[test]
    fn test_restore_with_line_number() {
        let output = run_program(
            "\
10 DATA 10, 20
20 DATA 30, 40
30 READ A, B, C, D
40 RESTORE 20
50 READ E, F
60 PRINT A; B; C; D
70 PRINT E; F
80 END
",
        )
        .unwrap();
        assert_eq!(output, " 10  20  30  40 \n 30  40 \n");
    }

    #[test]
    fn test_restore_without_data_at_line() {
        // RESTORE to a line after all DATA; next READ should fail
        let result = run_program(
            "\
10 DATA 1, 2
20 RESTORE 100
30 READ A
40 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Out of DATA"));
    }

    #[test]
    fn test_data_negative_numbers() {
        let output = run_program(
            "\
10 DATA -5, -3.14, 0, 42
20 READ A, B, C, D
30 PRINT A; B; C; D
40 END
",
        )
        .unwrap();
        assert_eq!(output, "-5 -3.14  0  42 \n");
    }

    #[test]
    fn test_data_not_executed_sequentially() {
        // DATA can appear after END; it's still collected
        let output = run_program(
            "\
10 READ A, B
20 PRINT A; B
30 END
40 DATA 99, 88
",
        )
        .unwrap();
        assert_eq!(output, " 99  88 \n");
    }

    #[test]
    fn test_data_on_same_line_as_other_statements() {
        let output = run_program(
            "\
10 X = 5 : DATA 10, 20
20 READ A, B
30 PRINT X; A; B
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 5  10  20 \n");
    }

    #[test]
    fn test_multiple_read_statements() {
        let output = run_program(
            "\
10 DATA 1, 2, 3, 4
20 READ A
30 READ B
40 READ C
50 READ D
60 PRINT A; B; C; D
70 END
",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3  4 \n");
    }

    #[test]
    fn test_restore_and_reread_loop() {
        let output = run_program(
            "\
10 DATA 10, 20, 30
20 FOR I = 1 TO 2
30 RESTORE
40 READ A, B, C
50 PRINT A + B + C;
60 NEXT I
70 END
",
        )
        .unwrap();
        assert_eq!(output, " 60  60 ");
    }

    #[test]
    fn test_read_type_mismatch_error() {
        // Trying to read a non-numeric string into a numeric variable
        let result = run_program(
            "\
10 DATA \"HELLO\"
20 READ A
30 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_data_unquoted_string() {
        // Unquoted identifiers in DATA become strings
        let output = run_program(
            "\
10 DATA HELLO, WORLD
20 READ A$, B$
30 PRINT A$; \" \"; B$
40 END
",
        )
        .unwrap();
        assert_eq!(output, "HELLO WORLD\n");
    }

    #[test]
    fn test_restore_to_first_data_line() {
        let output = run_program(
            "\
10 DATA 1
20 DATA 2
30 READ A
40 READ B
50 RESTORE 10
60 READ C
70 PRINT A; B; C
80 END
",
        )
        .unwrap();
        assert_eq!(output, " 1  2  1 \n");
    }

    #[test]
    fn test_data_read_single_value() {
        let output = run_program(
            "\
10 DATA 42
20 READ X
30 PRINT X
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_data_numeric_string_to_numeric_var() {
        // A numeric string in DATA can be read into a numeric variable
        let output = run_program(
            "\
10 DATA \"3.14\"
20 READ X
30 PRINT X
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 3.14 \n");
    }

    // --- DIM / ERASE / Array integration tests ---

    #[test]
    fn test_dim_and_array_access() {
        let output = run_program(
            "\
10 DIM A(5)
20 A(1) = 10
30 A(2) = 20
40 PRINT A(1); A(2)
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 10  20 \n");
    }

    #[test]
    fn test_dim_multidimensional() {
        let output = run_program(
            "\
10 DIM B(3, 4)
20 B(1, 2) = 42
30 PRINT B(1, 2)
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 42 \n");
    }

    #[test]
    fn test_dim_initialized_to_zero() {
        let output = run_program(
            "\
10 DIM A(5)
20 PRINT A(0); A(3); A(5)
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 0  0  0 \n");
    }

    #[test]
    fn test_dim_string_array() {
        let output = run_program(
            "\
10 DIM N$(3)
20 N$(1) = \"HELLO\"
30 N$(2) = \"WORLD\"
40 PRINT N$(1); \" \"; N$(2)
50 END
",
        )
        .unwrap();
        assert_eq!(output, "HELLO WORLD\n");
    }

    #[test]
    fn test_array_auto_dimension() {
        // Accessing array without DIM should auto-create with max subscript 10
        let output = run_program(
            "\
10 A(5) = 99
20 PRINT A(5)
30 END
",
        )
        .unwrap();
        assert_eq!(output, " 99 \n");
    }

    #[test]
    fn test_array_subscript_out_of_range() {
        let result = run_program(
            "\
10 DIM A(5)
20 A(6) = 1
30 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Subscript out of range"));
    }

    #[test]
    fn test_erase_and_redim() {
        let output = run_program(
            "\
10 DIM A(5)
20 A(3) = 42
30 ERASE A
40 DIM A(10)
50 PRINT A(3)
60 END
",
        )
        .unwrap();
        // After ERASE and re-DIM, all elements should be zero
        assert_eq!(output, " 0 \n");
    }

    #[test]
    fn test_redim_without_erase_error() {
        let result = run_program(
            "\
10 DIM A(5)
20 DIM A(10)
30 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already dimensioned"));
    }

    #[test]
    fn test_array_in_for_loop() {
        let output = run_program(
            "\
10 DIM A(5)
20 FOR I = 1 TO 5
30   A(I) = I * 10
40 NEXT I
50 FOR I = 1 TO 5
60   PRINT A(I);
70 NEXT I
80 PRINT
90 END
",
        )
        .unwrap();
        assert_eq!(output, " 10  20  30  40  50 \n");
    }

    #[test]
    fn test_array_in_expression() {
        let output = run_program(
            "\
10 DIM A(5)
20 A(1) = 10
30 A(2) = 20
40 PRINT A(1) + A(2)
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 30 \n");
    }

    #[test]
    fn test_read_into_array() {
        let output = run_program(
            "\
10 DIM A(5)
20 DATA 10, 20, 30
30 READ A(1), A(2), A(3)
40 PRINT A(1); A(2); A(3)
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 10  20  30 \n");
    }

    #[test]
    fn test_erase_multiple() {
        let output = run_program(
            "\
10 DIM A(5), B(3)
20 A(1) = 1
30 B(1) = 2
40 ERASE A, B
50 DIM A(10), B(10)
60 PRINT A(1); B(1)
70 END
",
        )
        .unwrap();
        assert_eq!(output, " 0  0 \n");
    }

    #[test]
    fn test_dim_3d_array() {
        let output = run_program(
            "\
10 DIM C(2, 3, 4)
20 C(1, 2, 3) = 123
30 PRINT C(1, 2, 3); C(0, 0, 0)
40 END
",
        )
        .unwrap();
        assert_eq!(output, " 123  0 \n");
    }

    #[test]
    fn test_array_with_computed_index() {
        let output = run_program(
            "\
10 DIM A(10)
20 I = 3
30 A(I * 2) = 99
40 PRINT A(6)
50 END
",
        )
        .unwrap();
        assert_eq!(output, " 99 \n");
    }

    #[test]
    fn test_gosub_return_basic() {
        let output = run_program(
            "\
10 PRINT \"BEFORE\"
20 GOSUB 100
30 PRINT \"AFTER\"
40 END
100 PRINT \"IN SUB\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "BEFORE\nIN SUB\nAFTER\n");
    }

    #[test]
    fn test_gosub_return_multiple_calls() {
        let output = run_program(
            "\
10 GOSUB 100
20 GOSUB 100
30 END
100 PRINT \"SUB\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "SUB\nSUB\n");
    }

    #[test]
    fn test_gosub_nested() {
        let output = run_program(
            "\
10 GOSUB 100
20 END
100 PRINT \"OUTER\"
110 GOSUB 200
120 PRINT \"OUTER DONE\"
130 RETURN
200 PRINT \"INNER\"
210 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "OUTER\nINNER\nOUTER DONE\n");
    }

    #[test]
    fn test_gosub_with_variables() {
        let output = run_program(
            "\
10 X = 5
20 GOSUB 100
30 PRINT X
40 END
100 X = X * 2
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, " 10 \n");
    }

    #[test]
    fn test_gosub_expression_target() {
        let output = run_program(
            "\
10 L = 100
20 GOSUB L
30 END
100 PRINT \"CALLED\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "CALLED\n");
    }

    #[test]
    fn test_gosub_on_multi_statement_line() {
        let output = run_program(
            "\
10 GOSUB 100 : PRINT \"AFTER GOSUB\"
20 END
100 PRINT \"IN SUB\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "IN SUB\nAFTER GOSUB\n");
    }

    #[test]
    fn test_return_without_gosub() {
        let result = run_program(
            "\
10 RETURN
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RETURN without GOSUB"));
    }

    #[test]
    fn test_gosub_invalid_line() {
        let result = run_program(
            "\
10 GOSUB 999
20 END
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_gosub_in_for_loop() {
        let output = run_program(
            "\
10 FOR I = 1 TO 3
20 GOSUB 100
30 NEXT I
40 END
100 PRINT I;
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3 ");
    }

    #[test]
    fn test_gosub_in_if_then() {
        let output = run_program(
            "\
10 X = 1
20 IF X = 1 THEN GOSUB 100
30 END
100 PRINT \"YES\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "YES\n");
    }

    #[test]
    fn test_gosub_deeply_nested() {
        let output = run_program(
            "\
10 GOSUB 100
20 END
100 GOSUB 200
110 RETURN
200 GOSUB 300
210 RETURN
300 PRINT \"DEEP\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "DEEP\n");
    }

    #[test]
    fn test_return_with_line_number() {
        let output = run_program(
            "\
10 GOSUB 100
20 PRINT \"SHOULD NOT PRINT\"
30 END
100 PRINT \"IN SUB\"
110 RETURN 30
",
        )
        .unwrap();
        assert_eq!(output, "IN SUB\n");
    }

    #[test]
    fn test_return_with_expression_target() {
        let output = run_program(
            "\
10 GOSUB 100
20 PRINT \"SHOULD NOT PRINT\"
30 END
100 PRINT \"IN SUB\"
110 L = 30
120 RETURN L
",
        )
        .unwrap();
        assert_eq!(output, "IN SUB\n");
    }

    #[test]
    fn test_return_with_line_number_redirects_flow() {
        let output = run_program(
            "\
10 GOSUB 100
20 PRINT \"NORMAL RETURN\"
30 END
50 PRINT \"REDIRECTED\"
60 END
100 PRINT \"IN SUB\"
110 RETURN 50
",
        )
        .unwrap();
        assert_eq!(output, "IN SUB\nREDIRECTED\n");
    }

    #[test]
    fn test_return_with_line_number_still_pops_stack() {
        // RETURN with line number should still pop the gosub stack,
        // so a subsequent RETURN should use the outer GOSUB's return address
        let output = run_program(
            "\
10 GOSUB 100
20 PRINT \"BACK FROM OUTER\"
30 END
100 PRINT \"OUTER\"
110 GOSUB 200
120 PRINT \"SHOULD NOT PRINT\"
130 RETURN
200 PRINT \"INNER\"
210 RETURN 130
",
        )
        .unwrap();
        // Inner RETURN 130 pops inner stack entry, jumps to line 130 (RETURN)
        // Line 130 RETURN pops outer stack entry, jumps back to line 20
        assert_eq!(output, "OUTER\nINNER\nBACK FROM OUTER\n");
    }

    #[test]
    fn test_return_with_invalid_line_number() {
        let result = run_program(
            "\
10 GOSUB 100
20 END
100 RETURN 999
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_return_without_gosub_with_line_number() {
        let result = run_program(
            "\
10 RETURN 100
",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RETURN without GOSUB"));
    }

    #[test]
    fn test_on_gosub_basic() {
        let output = run_program(
            "\
10 ON 1 GOSUB 100, 200, 300
20 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
300 PRINT \"THIRD\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "FIRST\n");
    }

    #[test]
    fn test_on_gosub_second_target() {
        let output = run_program(
            "\
10 ON 2 GOSUB 100, 200, 300
20 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
300 PRINT \"THIRD\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "SECOND\n");
    }

    #[test]
    fn test_on_gosub_third_target() {
        let output = run_program(
            "\
10 ON 3 GOSUB 100, 200, 300
20 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
300 PRINT \"THIRD\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "THIRD\n");
    }

    #[test]
    fn test_on_gosub_out_of_range_zero() {
        let output = run_program(
            "\
10 ON 0 GOSUB 100, 200
20 PRINT \"CONTINUED\"
30 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "CONTINUED\n");
    }

    #[test]
    fn test_on_gosub_out_of_range_high() {
        let output = run_program(
            "\
10 ON 5 GOSUB 100, 200
20 PRINT \"CONTINUED\"
30 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "CONTINUED\n");
    }

    #[test]
    fn test_on_gosub_negative_index() {
        let output = run_program(
            "\
10 ON -1 GOSUB 100, 200
20 PRINT \"CONTINUED\"
30 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "CONTINUED\n");
    }

    #[test]
    fn test_on_gosub_expression_selector() {
        let output = run_program(
            "\
10 X = 1
20 ON X + 1 GOSUB 100, 200, 300
30 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
300 PRINT \"THIRD\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "SECOND\n");
    }

    #[test]
    fn test_on_gosub_in_loop() {
        let output = run_program(
            "\
10 FOR I = 1 TO 3
20   ON I GOSUB 100, 200, 300
30 NEXT I
40 END
100 PRINT \"A\";
110 RETURN
200 PRINT \"B\";
210 RETURN
300 PRINT \"C\"
310 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "ABC\n");
    }

    #[test]
    fn test_on_gosub_multi_statement_line() {
        let output = run_program(
            "\
10 R$ = \"NONE\" : ON 2 GOSUB 100, 200 : PRINT R$
20 END
100 R$ = \"FIRST\"
110 RETURN
200 R$ = \"SECOND\"
210 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "SECOND\n");
    }

    #[test]
    fn test_on_gosub_with_return_to_multi_statement() {
        // Verify that RETURN from ON GOSUB correctly resumes at the next
        // statement on the same line as the ON GOSUB
        let output = run_program(
            "\
10 ON 1 GOSUB 100 : PRINT \"AFTER\"
20 END
100 PRINT \"SUB\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "SUB\nAFTER\n");
    }

    #[test]
    fn test_on_gosub_single_target() {
        let output = run_program(
            "\
10 ON 1 GOSUB 100
20 END
100 PRINT \"ONLY\"
110 RETURN
",
        )
        .unwrap();
        assert_eq!(output, "ONLY\n");
    }

    #[test]
    fn test_end_after_for_next_on_same_line() {
        let output = run_program("10 FOR I = 1 TO 3 : PRINT \"X\"; : NEXT I : END\n20 PRINT \"BAD\"\n").unwrap();
        assert_eq!(output, "XXX");
    }

    #[test]
    fn test_end_after_for_next_on_same_line_with_print() {
        let output =
            run_program("10 FOR I = 1 TO 3 : PRINT I; : NEXT I : END\n20 PRINT \"SHOULD NOT PRINT\"\n").unwrap();
        assert_eq!(output, " 1  2  3 ");
    }

    #[test]
    fn test_for_next_same_line_loop_body_executes_correctly() {
        let output = run_program("10 FOR I = 1 TO 3 : PRINT I : NEXT I\n").unwrap();
        assert_eq!(output, " 1 \n 2 \n 3 \n");
    }

    #[test]
    fn test_end_in_multistatement_line_stops_execution() {
        let output = run_program("10 PRINT \"A\" : END : PRINT \"B\"\n20 PRINT \"C\"\n").unwrap();
        assert_eq!(output, "A\n");
    }

    #[test]
    fn test_for_next_same_line_with_end_empty_print() {
        let output = run_program("10 FOR I = 1 TO 3 : PRINT : NEXT I : END\n20 PRINT \"BAD\"\n").unwrap();
        assert_eq!(output, "\n\n\n");
    }

    #[test]
    fn test_for_next_same_line_skip_when_empty_range() {
        let output = run_program("10 FOR I = 5 TO 1 : PRINT \"SKIP\" : NEXT I : PRINT \"AFTER\"\n").unwrap();
        assert_eq!(output, "AFTER\n");
    }

    #[test]
    fn test_nested_for_next_same_line_with_end() {
        let output = run_program(
            "10 S = 0\n20 FOR I = 1 TO 2 : FOR J = 1 TO 2 : S = S + 1 : NEXT J : NEXT I : PRINT S : END\n30 PRINT \"BAD\"\n",
        )
        .unwrap();
        assert_eq!(output, " 4 \n");
    }

    #[test]
    fn test_gosub_return_same_line_continues_after_gosub() {
        // RETURN should resume at the PRINT "AFTER" statement on the same line as GOSUB
        let output =
            run_program("10 GOSUB 100 : PRINT \"AFTER\" : END\n20 PRINT \"BAD\"\n100 PRINT \"SUB\"\n110 RETURN\n")
                .unwrap();
        assert_eq!(output, "SUB\nAFTER\n");
    }

    #[test]
    fn test_gosub_return_same_line_end_after_gosub() {
        // END on the same line after GOSUB should terminate after the subroutine returns
        let output = run_program("10 GOSUB 100 : END\n20 PRINT \"BAD\"\n100 PRINT \"SUB\"\n110 RETURN\n").unwrap();
        assert_eq!(output, "SUB\n");
    }

    #[test]
    fn test_multiple_gosubs_on_same_line() {
        // Multiple GOSUBs on the same line should each return to the correct position
        let output = run_program(
            "10 GOSUB 100 : GOSUB 200 : PRINT \"DONE\" : END\n20 PRINT \"BAD\"\n100 PRINT \"A\"\n110 RETURN\n200 PRINT \"B\"\n210 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "A\nB\nDONE\n");
    }

    #[test]
    fn test_on_gosub_return_same_line_continues_after_gosub() {
        // ON...GOSUB should return to the statement after the ON GOSUB on the same line
        let output = run_program(
            "10 ON 2 GOSUB 100, 200, 300 : PRINT \"AFTER\" : END\n20 PRINT \"BAD\"\n100 PRINT \"S1\"\n110 RETURN\n200 PRINT \"S2\"\n210 RETURN\n300 PRINT \"S3\"\n310 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "S2\nAFTER\n");
    }

    #[test]
    fn test_on_gosub_return_same_line_with_end() {
        // END after ON...GOSUB on the same line should terminate after subroutine returns
        let output = run_program("10 ON 1 GOSUB 100 : END\n20 PRINT \"BAD\"\n100 PRINT \"SUB\"\n110 RETURN\n").unwrap();
        assert_eq!(output, "SUB\n");
    }

    #[test]
    fn test_gosub_return_nested_same_line() {
        // Nested GOSUB calls from multi-statement lines should maintain correct return addresses
        let output = run_program(
            "10 GOSUB 100 : PRINT \"BACK1\" : END\n100 GOSUB 200 : PRINT \"BACK2\"\n110 RETURN\n200 PRINT \"DEEP\"\n210 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "DEEP\nBACK2\nBACK1\n");
    }

    // =========================================================================
    // Tests for nested combinations of IF/THEN/ELSE, GOSUB/RETURN,
    // ON...GOSUB/RETURN, GOTO, and END on multi-statement lines
    // =========================================================================

    #[test]
    fn test_if_then_gosub_on_multistatement_line() {
        // IF true -> GOSUB executes, RETURN resumes at statement after the IF
        let output =
            run_program("10 X = 1 : IF X = 1 THEN GOSUB 100 : PRINT \"AFTER\" : END\n100 PRINT \"SUB\"\n110 RETURN\n")
                .unwrap();
        assert_eq!(output, "SUB\nAFTER\n");
    }

    #[test]
    fn test_if_false_then_gosub_skips_rest_of_line() {
        // IF false with no ELSE -> SkipLine, remaining statements on line are skipped
        let output = run_program(
            "10 IF 0 THEN GOSUB 100 : PRINT \"SHOULD NOT PRINT\"\n20 PRINT \"NEXT LINE\"\n30 END\n100 PRINT \"SUB\"\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "NEXT LINE\n");
    }

    #[test]
    fn test_if_then_goto_on_multistatement_line() {
        // IF true -> GOTO skips remaining statements on the line
        let output =
            run_program("10 IF 1 THEN GOTO 30 : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n30 PRINT \"OK\"\n").unwrap();
        assert_eq!(output, "OK\n");
    }

    #[test]
    fn test_if_false_then_goto_else_goto_on_multistatement_line() {
        // IF false -> ELSE GOTO, remaining statements skipped
        let output = run_program(
            "10 IF 0 THEN GOTO 30 ELSE GOTO 40 : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n30 PRINT \"THEN\" : END\n40 PRINT \"ELSE\"\n",
        )
        .unwrap();
        assert_eq!(output, "ELSE\n");
    }

    #[test]
    fn test_if_then_end_on_multistatement_line() {
        // IF true -> END terminates immediately, remaining statements skipped
        let output = run_program("10 IF 1 THEN END : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n").unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_if_else_end_on_multistatement_line() {
        // IF false -> ELSE END terminates immediately
        let output =
            run_program("10 PRINT \"BEFORE\" : IF 0 THEN PRINT \"T\" ELSE END : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n")
                .unwrap();
        assert_eq!(output, "BEFORE\n");
    }

    #[test]
    fn test_if_else_gosub_on_multistatement_line() {
        // IF false -> ELSE GOSUB, RETURN resumes at statement after the IF
        let output = run_program(
            "10 IF 0 THEN PRINT \"T\" ELSE GOSUB 100 : PRINT \"AFTER\" : END\n100 PRINT \"ELSESUB\"\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "ELSESUB\nAFTER\n");
    }

    #[test]
    fn test_gosub_then_if_goto_on_same_line() {
        // GOSUB returns, then IF...THEN GOTO executes on same line
        let output = run_program(
            "10 GOSUB 100 : IF X = 42 THEN GOTO 30 : PRINT \"SKIP\"\n20 PRINT \"SKIP2\" : END\n30 PRINT \"JUMPED\" : END\n100 X = 42\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "JUMPED\n");
    }

    #[test]
    fn test_gosub_then_if_else_on_same_line() {
        // GOSUB sets a variable, then IF...ELSE decides based on it
        let output = run_program(
            "10 GOSUB 100 : IF X = 99 THEN PRINT \"YES\" ELSE PRINT \"NO\" : END\n100 X = 99\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "YES\n");
    }

    #[test]
    fn test_on_gosub_inside_if_then_on_multistatement_line() {
        // IF true -> ON...GOSUB, RETURN resumes at statement after the IF
        let output = run_program(
            "10 X = 2 : IF X > 0 THEN ON X GOSUB 100, 200 : PRINT \"DONE\" : END\n100 PRINT \"S1\" : RETURN\n200 PRINT \"S2\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "S2\nDONE\n");
    }

    #[test]
    fn test_on_gosub_then_goto_on_same_line() {
        // ON...GOSUB returns, then GOTO on the same line
        let output = run_program(
            "10 ON 1 GOSUB 100 : GOTO 30 : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n30 PRINT \"JUMPED\" : END\n100 PRINT \"SUB\"\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "SUB\nJUMPED\n");
    }

    #[test]
    fn test_on_gosub_then_if_then_end_on_same_line() {
        // ON...GOSUB returns, then IF...THEN END terminates
        let output = run_program(
            "10 ON 1 GOSUB 100 : IF 1 THEN END : PRINT \"SKIP\"\n20 PRINT \"SKIP2\"\n100 PRINT \"SUB\"\n110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "SUB\n");
    }

    #[test]
    fn test_multiple_gosubs_with_if_on_same_line() {
        // Multiple GOSUBs interspersed with IF on the same line
        let output = run_program(
            "10 GOSUB 100 : IF X = 1 THEN GOSUB 200 : PRINT \"DONE\" : END\n100 X = 1 : RETURN\n200 PRINT \"SECOND\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "SECOND\nDONE\n");
    }

    #[test]
    fn test_goto_from_subroutine_on_multistatement_line() {
        // GOTO inside a subroutine on a multi-statement line
        let output = run_program(
            "10 GOSUB 100 : PRINT \"BACK\" : END\n100 PRINT \"A\" : GOTO 110 : PRINT \"SKIP\"\n110 PRINT \"B\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "A\nB\nBACK\n");
    }

    #[test]
    fn test_if_then_gosub_nested_in_subroutine() {
        // Subroutine contains IF...THEN GOSUB on a multi-statement line (nested GOSUB)
        let output = run_program(
            "10 GOSUB 100 : PRINT \"FINAL\" : END\n100 IF 1 THEN GOSUB 200 : PRINT \"MID\" : RETURN\n200 PRINT \"DEEP\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "DEEP\nMID\nFINAL\n");
    }

    #[test]
    fn test_end_inside_if_in_subroutine_on_multistatement_line() {
        // END inside IF within a subroutine terminates the entire program
        let output = run_program(
            "10 GOSUB 100 : PRINT \"SHOULD NOT PRINT\"\n100 PRINT \"IN SUB\" : IF 1 THEN END : PRINT \"SKIP\"\n",
        )
        .unwrap();
        assert_eq!(output, "IN SUB\n");
    }

    #[test]
    fn test_return_with_line_number_from_multistatement_line() {
        // RETURN with a line number redirects instead of returning to caller
        let output = run_program(
            "10 GOSUB 100 : PRINT \"SHOULD NOT PRINT\" : END\n20 PRINT \"REDIRECTED\" : END\n100 PRINT \"SUB\" : RETURN 20\n",
        )
        .unwrap();
        assert_eq!(output, "SUB\nREDIRECTED\n");
    }

    #[test]
    fn test_on_gosub_out_of_range_continues_on_multistatement_line() {
        // ON...GOSUB with out-of-range selector should continue to next statement
        let output = run_program(
            "10 ON 5 GOSUB 100, 200 : PRINT \"CONTINUED\" : END\n100 PRINT \"S1\" : RETURN\n200 PRINT \"S2\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "CONTINUED\n");
    }

    #[test]
    fn test_if_then_goto_else_gosub_on_multistatement_line() {
        // True: GOTO jumps; False: GOSUB and return to same line
        // Test the false (ELSE) branch
        let output = run_program(
            "10 IF 0 THEN GOTO 30 ELSE GOSUB 100 : PRINT \"AFTER ELSE\" : END\n30 PRINT \"THEN\" : END\n100 PRINT \"ELSESUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "ELSESUB\nAFTER ELSE\n");
    }

    #[test]
    fn test_if_then_goto_else_gosub_true_branch() {
        // True: GOTO jumps, skipping rest of line
        let output = run_program(
            "10 IF 1 THEN GOTO 30 ELSE GOSUB 100 : PRINT \"SKIP\" : END\n30 PRINT \"THEN\" : END\n100 PRINT \"ELSESUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "THEN\n");
    }

    #[test]
    fn test_chained_gosub_goto_end_on_multistatement_line() {
        // GOSUB returns, GOTO jumps, END terminates (3 control flow ops on one line)
        let output = run_program(
            "10 GOSUB 100 : GOTO 20 : PRINT \"SKIP\"\n20 PRINT \"JUMPED\" : END\n100 PRINT \"SUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "SUB\nJUMPED\n");
    }

    #[test]
    fn test_if_false_skips_line_but_not_next_line() {
        // IF false with no ELSE skips the rest of the current line only
        let output = run_program("10 PRINT \"A\" : IF 0 THEN PRINT \"B\" : PRINT \"C\"\n20 PRINT \"D\"\n").unwrap();
        assert_eq!(output, "A\nD\n");
    }

    #[test]
    fn test_deeply_nested_gosub_with_if_and_goto() {
        // 3-level nested GOSUB with IF and GOTO mixed in
        let output = run_program(
            "10 GOSUB 100 : PRINT \"L0\" : END\n\
             100 IF 1 THEN GOSUB 200 : PRINT \"L1\" : RETURN\n\
             200 GOSUB 300 : IF 1 THEN GOTO 210 : PRINT \"SKIP\"\n\
             210 PRINT \"L2\" : RETURN\n\
             300 PRINT \"L3\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "L3\nL2\nL1\nL0\n");
    }

    #[test]
    fn test_on_gosub_with_if_decision_after_return() {
        // ON...GOSUB sets a variable, IF after return uses it to branch
        let output = run_program(
            "10 V = 2 : ON V GOSUB 100, 200, 300 : IF R = 20 THEN PRINT \"CORRECT\" ELSE PRINT \"WRONG\" : END\n\
             100 R = 10 : RETURN\n\
             200 R = 20 : RETURN\n\
             300 R = 30 : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "CORRECT\n");
    }

    #[test]
    fn test_gosub_return_to_end_on_same_line_no_extra_execution() {
        // After GOSUB returns, END is the very next statement—nothing else should run
        let output = run_program(
            "10 PRINT \"START\" : GOSUB 100 : END : PRINT \"SHOULD NOT PRINT\"\n20 PRINT \"ALSO SHOULD NOT PRINT\"\n100 PRINT \"SUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "START\nSUB\n");
    }

    #[test]
    fn test_if_then_on_gosub_else_goto_multistatement() {
        // IF true -> ON...GOSUB; false would GOTO
        let output = run_program(
            "10 X = 1 : IF X = 1 THEN ON X GOSUB 100 : PRINT \"DONE\" : END\n30 PRINT \"ELSE\" : END\n100 PRINT \"ON-SUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "ON-SUB\nDONE\n");
    }

    #[test]
    fn test_goto_inside_if_inside_subroutine_multistatement() {
        // Subroutine has IF...THEN GOTO on multi-statement line
        let output = run_program(
            "10 X = 5 : GOSUB 100 : PRINT \"BACK\" : END\n\
             100 IF X > 3 THEN GOTO 110 : PRINT \"SKIP\"\n\
             110 PRINT \"JUMPED IN SUB\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "JUMPED IN SUB\nBACK\n");
    }

    #[test]
    fn test_randomized_on_gosub_with_end_on_multistatement_line() {
        // Use a randomized selector to test ON...GOSUB dispatch, each subroutine
        // prints its index, returns, and END follows on the same line
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for selector in 1..=3 {
            let program = format!(
                "10 ON {} GOSUB 100, 200, 300 : END\n\
                 100 PRINT \"1\" : RETURN\n\
                 200 PRINT \"2\" : RETURN\n\
                 300 PRINT \"3\" : RETURN\n",
                selector
            );
            let output = run_program(&program).unwrap();
            let expected = format!("{}\n", selector);
            assert_eq!(output, expected, "ON {} GOSUB dispatched incorrectly", selector);
            seen.insert(selector);
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn test_randomized_if_gosub_branching_on_multistatement_line() {
        // Randomized: test multiple threshold values with IF...THEN GOSUB vs ELSE GOSUB
        for x in 0..=10 {
            let program = format!(
                "10 X = {} : IF X > 5 THEN GOSUB 100 ELSE GOSUB 200 : END\n\
                 100 PRINT \"HIGH\" : RETURN\n\
                 200 PRINT \"LOW\" : RETURN\n",
                x
            );
            let output = run_program(&program).unwrap();
            if x > 5 {
                assert_eq!(output, "HIGH\n", "X={} should be HIGH", x);
            } else {
                assert_eq!(output, "LOW\n", "X={} should be LOW", x);
            }
        }
    }

    #[test]
    fn test_for_next_with_gosub_on_same_multistatement_line() {
        // FOR loop body contains GOSUB on the same line
        let output = run_program(
            "10 FOR I = 1 TO 3 : GOSUB 100 : NEXT I : PRINT \"DONE\" : END\n\
             100 PRINT I; : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3 DONE\n");
    }

    #[test]
    fn test_for_next_with_if_goto_early_exit() {
        // FOR loop with IF...THEN GOTO (early exit) — NEXT must be on its own line
        // because IF false (no ELSE) causes SkipLine, skipping remaining statements
        let output = run_program(
            "10 FOR I = 1 TO 10\n\
             20 IF I = 4 THEN GOTO 50\n\
             30 PRINT I;\n\
             40 NEXT I\n\
             50 PRINT \"EXITED AT\"; I\n",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3 EXITED AT 4 \n");
    }

    #[test]
    fn test_for_next_with_conditional_gosub_separate_lines() {
        // FOR loop with conditional GOSUB — IF without ELSE causes SkipLine
        // which skips remaining statements, so NEXT must be on a separate line
        let output = run_program(
            "10 FOR I = 1 TO 4\n\
             20 IF I > 2 THEN GOSUB 100\n\
             30 NEXT I\n\
             40 PRINT \"DONE\" : END\n\
             100 PRINT \"CALLED\"; I : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "CALLED 3 \nCALLED 4 \nDONE\n");
    }

    #[test]
    fn test_for_next_with_conditional_gosub_using_else_on_same_line() {
        // IF with ELSE avoids SkipLine, so NEXT can be on the same line
        let output = run_program(
            "10 FOR I = 1 TO 4 : IF I > 2 THEN GOSUB 100 ELSE PRINT \"\"; : NEXT I : PRINT \"DONE\" : END\n\
             100 PRINT \"CALLED\"; I; : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "CALLED 3 CALLED 4 DONE\n");
    }

    #[test]
    fn test_gosub_with_return_line_number_on_multistatement_line() {
        // RETURN with explicit line number on a multi-statement line
        let output = run_program(
            "10 PRINT \"A\" : GOSUB 100 : PRINT \"SHOULD NOT PRINT\" : END\n\
             20 PRINT \"REDIRECTED\" : END\n\
             100 PRINT \"SUB\" : RETURN 20 : PRINT \"SKIP\"\n",
        )
        .unwrap();
        assert_eq!(output, "A\nSUB\nREDIRECTED\n");
    }

    #[test]
    fn test_nested_on_gosub_on_multistatement_lines() {
        // ON...GOSUB from within a subroutine that was itself called by ON...GOSUB
        let output = run_program(
            "10 ON 1 GOSUB 100 : PRINT \"OUTER DONE\" : END\n\
             100 PRINT \"L1\" : ON 2 GOSUB 200, 300 : PRINT \"L1 DONE\" : RETURN\n\
             200 PRINT \"L2A\" : RETURN\n\
             300 PRINT \"L2B\" : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "L1\nL2B\nL1 DONE\nOUTER DONE\n");
    }

    #[test]
    fn test_if_then_line_number_on_multistatement_line() {
        // IF...THEN <line_number> is an implicit GOTO on a multi-statement line
        let output = run_program(
            "10 PRINT \"BEFORE\" : IF 1 THEN 30 : PRINT \"SKIP\"\n\
             20 PRINT \"SKIP2\"\n\
             30 PRINT \"TARGET\"\n",
        )
        .unwrap();
        assert_eq!(output, "BEFORE\nTARGET\n");
    }

    #[test]
    fn test_if_else_line_number_on_multistatement_line() {
        // IF false -> ELSE <line_number> (implicit GOTO) on multi-statement line
        let output = run_program(
            "10 PRINT \"BEFORE\" : IF 0 THEN 30 ELSE 40 : PRINT \"SKIP\"\n\
             30 PRINT \"THEN\" : END\n\
             40 PRINT \"ELSE\"\n",
        )
        .unwrap();
        assert_eq!(output, "BEFORE\nELSE\n");
    }

    #[test]
    fn test_gosub_from_for_loop_with_end_in_subroutine() {
        // GOSUB from a FOR loop; subroutine conditionally ENDs
        let output = run_program(
            "10 FOR I = 1 TO 5\n\
             20 GOSUB 100 : NEXT I\n\
             30 PRINT \"DONE\" : END\n\
             100 PRINT I; : IF I = 3 THEN END\n\
             110 RETURN\n",
        )
        .unwrap();
        assert_eq!(output, " 1  2  3 ");
    }

    #[test]
    fn test_multiple_control_flow_ops_one_line() {
        // LET, GOSUB, IF, GOTO, END all on one multi-statement line
        let output = run_program(
            "10 X = 0 : GOSUB 100 : IF X = 1 THEN GOTO 20 : PRINT \"SKIP\"\n\
             20 PRINT \"REACHED\" : END\n\
             100 X = 1 : RETURN\n",
        )
        .unwrap();
        assert_eq!(output, "REACHED\n");
    }

    // ===== CLS tests =====

    #[test]
    fn test_cls_no_args() {
        let output = run_program("10 CLS\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_cls_zero() {
        let output = run_program("10 CLS 0\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_cls_two() {
        let output = run_program("10 CLS 2\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_cls_one() {
        let output = run_program("10 CLS 1\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_cls_invalid_mode() {
        let result = run_program("10 CLS 3\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_cls_resets_column() {
        let output = run_program("10 PRINT \"HI\";\n20 CLS\n30 PRINT \"A\",\"B\"\n40 END\n").unwrap();
        // After CLS, column resets to 0, so comma tab works from column 0
        assert!(output.contains("\x1b[2J\x1b[H"));
        assert!(output.contains("A"));
    }

    // ===== LOCATE tests =====

    #[test]
    fn test_locate_row_col() {
        let output = run_program("10 LOCATE 1, 1\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[1;1H");
    }

    #[test]
    fn test_locate_row_col_mid_screen() {
        let output = run_program("10 LOCATE 12, 40\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[12;40H");
    }

    #[test]
    fn test_locate_row_only() {
        let output = run_program("10 LOCATE 5\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[5;1H");
    }

    #[test]
    fn test_locate_col_only() {
        let output = run_program("10 LOCATE ,10\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[10G");
    }

    #[test]
    fn test_locate_cursor_visible() {
        let output = run_program("10 LOCATE ,,1\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[?25h");
    }

    #[test]
    fn test_locate_cursor_hidden() {
        let output = run_program("10 LOCATE ,,0\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[?25l");
    }

    #[test]
    fn test_locate_invalid_row_zero() {
        let result = run_program("10 LOCATE 0, 1\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_locate_invalid_row_26() {
        let result = run_program("10 LOCATE 26, 1\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_locate_invalid_col_zero() {
        let result = run_program("10 LOCATE 1, 0\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_locate_invalid_col_81() {
        let result = run_program("10 LOCATE 1, 81\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_locate_with_print() {
        let output = run_program("10 LOCATE 5, 10\n20 PRINT \"HELLO\"\n30 END\n").unwrap();
        assert_eq!(output, "\x1b[5;10HHELLO\x1b[K\n");
    }

    #[test]
    fn test_locate_no_args() {
        let output = run_program("10 LOCATE\n20 END\n").unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_locate_with_scan_lines() {
        let output = run_program("10 LOCATE 5, 1, 1, 0, 7\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[5;1H\x1b[?25h");
    }

    #[test]
    fn test_locate_invalid_start_scan() {
        let result = run_program("10 LOCATE ,,,32\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_locate_invalid_stop_scan() {
        let result = run_program("10 LOCATE ,,,,32\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    // ===== COLOR tests =====

    #[test]
    fn test_color_default_white_on_black() {
        let output = run_program("10 COLOR 7, 0\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[37;40m");
    }

    #[test]
    fn test_color_red_on_blue() {
        let output = run_program("10 COLOR 4, 1\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[31;44m");
    }

    #[test]
    fn test_color_bright_yellow_on_green() {
        let output = run_program("10 COLOR 14, 2\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[93;42m");
    }

    #[test]
    fn test_color_blinking() {
        let output = run_program("10 COLOR 23, 0\n20 END\n").unwrap();
        // 23 = 7 + 16 (blinking white)
        assert_eq!(output, "\x1b[37;5;40m");
    }

    #[test]
    fn test_color_fg_only() {
        let output = run_program("10 COLOR 1\n20 END\n").unwrap();
        // Sets foreground to blue, background stays default (0=black)
        assert_eq!(output, "\x1b[34;40m");
    }

    #[test]
    fn test_color_with_border() {
        let output = run_program("10 COLOR 7, 0, 3\n20 END\n").unwrap();
        // Border is stored but only fg/bg emitted via ANSI (border is CGA-specific)
        assert_eq!(output, "\x1b[37;40m");
    }

    #[test]
    fn test_color_invalid_fg_too_high() {
        let result = run_program("10 COLOR 32\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_color_invalid_fg_negative() {
        let result = run_program("10 COLOR -1\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_color_invalid_bg_too_high() {
        let result = run_program("10 COLOR 7, 8\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_color_invalid_border_too_high() {
        let result = run_program("10 COLOR 7, 0, 16\n20 END\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Illegal function call"));
    }

    #[test]
    fn test_color_no_args_resets() {
        let output = run_program("10 COLOR\n20 END\n").unwrap();
        // No args: emit current defaults (fg=7, bg=0)
        assert_eq!(output, "\x1b[37;40m");
    }

    #[test]
    fn test_color_omitted_fg() {
        // COLOR ,2 — foreground unchanged (default 7), background = 2
        let output = run_program("10 COLOR ,2\n20 END\n").unwrap();
        assert_eq!(output, "\x1b[37;42m");
    }

    #[test]
    fn test_color_all_16_fg_colors() {
        // Verify all 16 basic foreground colors produce valid ANSI output
        for i in 0..16 {
            let prog = format!("10 COLOR {}\n20 END\n", i);
            let result = run_program(&prog);
            assert!(result.is_ok(), "COLOR {} should succeed", i);
            let output = result.unwrap();
            assert!(output.starts_with("\x1b["), "COLOR {} should produce ANSI escape", i);
        }
    }

    #[test]
    fn test_color_then_print() {
        let output = run_program("10 COLOR 1, 0\n20 PRINT \"HI\"\n30 END\n").unwrap();
        assert_eq!(output, "\x1b[34;40mHI\n");
    }

    #[test]
    fn test_cls_then_locate_then_print() {
        let output = run_program("10 CLS\n20 LOCATE 1, 1\n30 PRINT \"HELLO\"\n40 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H\x1b[1;1HHELLO\x1b[K\n");
    }

    #[test]
    fn test_color_locate_print_combo() {
        let output = run_program("10 COLOR 14, 1\n20 LOCATE 10, 20\n30 PRINT \"TEST\"\n40 END\n").unwrap();
        assert_eq!(output, "\x1b[93;44m\x1b[10;20HTEST\x1b[K\n");
    }

    #[test]
    fn test_locate_in_loop() {
        let output = run_program("10 FOR I = 1 TO 3\n20 LOCATE I, 1\n30 PRINT I;\n40 NEXT I\n50 END\n").unwrap();
        assert!(output.contains("\x1b[1;1H"));
        assert!(output.contains("\x1b[2;1H"));
        assert!(output.contains("\x1b[3;1H"));
    }

    #[test]
    fn test_cls_in_if_then() {
        let output = run_program("10 X = 1\n20 IF X = 1 THEN CLS\n30 END\n").unwrap();
        assert_eq!(output, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_color_in_if_then() {
        let output = run_program("10 X = 1\n20 IF X = 1 THEN COLOR 4, 0\n30 END\n").unwrap();
        assert_eq!(output, "\x1b[31;40m");
    }

    #[test]
    fn test_locate_with_expression() {
        let output = run_program("10 R = 5\n20 C = 10\n30 LOCATE R, C\n40 END\n").unwrap();
        assert_eq!(output, "\x1b[5;10H");
    }

    #[test]
    fn test_color_with_expression() {
        let output = run_program("10 FG = 14\n20 BG = 1\n30 COLOR FG, BG\n40 END\n").unwrap();
        assert_eq!(output, "\x1b[93;44m");
    }

    // ===== Clear-to-EOL behavior tests =====

    #[test]
    fn test_print_no_clear_eol_without_screen_commands() {
        // Without LOCATE/CLS, PRINT should not emit \x1b[K
        let output = run_program("10 PRINT \"HELLO\"\n20 END\n").unwrap();
        assert_eq!(output, "HELLO\n");
        assert!(!output.contains("\x1b[K"));
    }

    #[test]
    fn test_print_clear_eol_after_locate() {
        // After LOCATE, PRINT newlines should include \x1b[K to clear remaining line
        let output = run_program("10 LOCATE 1, 1\n20 PRINT \"HI\"\n30 END\n").unwrap();
        assert!(output.contains("\x1b[K\n"));
    }

    #[test]
    fn test_print_clear_eol_after_cls() {
        // After CLS, PRINT newlines should include \x1b[K
        let output = run_program("10 CLS\n20 PRINT \"HI\"\n30 END\n").unwrap();
        assert!(output.contains("\x1b[K\n"));
    }

    #[test]
    fn test_empty_print_clear_eol_after_locate() {
        // Empty PRINT (blank line) should also clear to EOL after screen commands
        let output = run_program("10 LOCATE 1, 1\n20 PRINT\n30 END\n").unwrap();
        assert!(output.contains("\x1b[K\n"));
    }

    #[test]
    fn test_print_semicolon_no_clear_eol() {
        // PRINT with trailing semicolon (no newline) should not emit \x1b[K
        let output = run_program("10 LOCATE 1, 1\n20 PRINT \"HI\";\n30 END\n").unwrap();
        assert!(!output.contains("\x1b[K"));
    }

    #[test]
    fn test_multiple_prints_all_clear_eol() {
        // All PRINT newlines after LOCATE should clear EOL
        let output = run_program("10 LOCATE 1, 1\n20 PRINT \"A\"\n30 PRINT \"B\"\n40 END\n").unwrap();
        assert_eq!(output, "\x1b[1;1HA\x1b[K\nB\x1b[K\n");
    }
}
