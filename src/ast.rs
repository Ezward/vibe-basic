//! Abstract Syntax Tree (AST) definitions and parser for BASIC programs.
//!
//! This module defines the AST node types (`Statement`, `PrintItem`, `ThenClause`,
//! `Line`, `Program`) that represent a parsed BASIC program, and provides the
//! `Parser` struct which converts a token stream into the AST. The parser handles
//! line numbers, multi-statement lines (colon-separated), and all BASIC statement
//! types including LET (explicit and implicit), PRINT, IF/THEN, GOTO, INPUT,
//! FOR/NEXT, GOSUB/RETURN, REM, and END.

use crate::expr::{Expr, ExprParser};
use crate::token::Token;

/// AST node representing a single BASIC statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Let {
        variable: String,
        indices: Vec<Expr>,
        expression: Expr,
    },
    Print {
        items: Vec<PrintItem>,
    },
    If {
        condition: Expr,
        then: Box<ThenClause>,
        else_clause: Option<Box<ThenClause>>,
    },
    Goto {
        line_number: u32,
    },
    Input {
        prompt: Option<String>,
        variable: String,
        indices: Vec<Expr>,
        suppress_question_mark: bool,
    },
    For {
        variable: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
    },
    Next {
        variable: Option<String>,
    },
    Data {
        values: Vec<Expr>,
    },
    Read {
        variables: Vec<(String, Vec<Expr>)>,
    },
    Restore {
        line_number: Option<u32>,
    },
    Rem(String),
    End,
    DefFn {
        name: String,
        params: Vec<String>,
        body: Expr,
    },
    Dim {
        arrays: Vec<(String, Vec<Expr>)>,
    },
    Erase {
        arrays: Vec<String>,
    },
    Gosub {
        target: Expr,
    },
    OnGosub {
        selector: Expr,
        targets: Vec<u32>,
    },
    Return {
        target: Option<Expr>,
    },
    /// LOCATE [row][,[col][,[cursor][,[start][,stop]]]]
    Locate {
        row: Option<Expr>,
        col: Option<Expr>,
        cursor: Option<Expr>,
        start: Option<Expr>,
        stop: Option<Expr>,
    },
    /// CLS [n] — clear screen (text mode only, n=0 or n=2 or no argument)
    Cls {
        mode: Option<Expr>,
    },
    /// COLOR [foreground][,[background][,border]] — set text colors (SCREEN 0)
    Color {
        foreground: Option<Expr>,
        background: Option<Expr>,
        border: Option<Expr>,
    },
}

/// Represents an item within a PRINT statement's output list.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintItem {
    Expression(Expr),
    Semicolon,
    Comma,
}

/// Represents the target of an IF/THEN: either a line number or an inline statement.
#[derive(Debug, Clone, PartialEq)]
pub enum ThenClause {
    LineNumber(u32),
    Statement(Box<Statement>),
}

/// A parsed BASIC line: line number + one or more statements
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub line_number: u32,
    pub statements: Vec<Statement>,
    /// 1-based source file line number where this BASIC line appeared
    pub source_line: usize,
}

/// A complete BASIC program
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub lines: Vec<Line>,
    /// Original source lines (0-indexed) for error reporting
    pub source_lines: Vec<String>,
}

/// Parser for BASIC programs
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// Current 1-based source file line number
    source_line: usize,
    /// Original source lines for error context
    source_lines: Vec<String>,
}

impl<'a> Parser<'a> {
    /// Creates a new parser from a token slice and the original source lines (for error messages).
    pub fn new(tokens: &'a [Token], source_lines: Vec<String>) -> Self {
        Parser {
            tokens,
            pos: 0,
            source_line: 1,
            source_lines,
        }
    }

    /// Format an error message with source line context
    fn error_with_context(&self, msg: String) -> String {
        let line_text = self
            .source_lines
            .get(self.source_line - 1)
            .map(|s| s.as_str())
            .unwrap_or("<unknown>");
        format!("{}\n  at line {}: {}", msg, self.source_line, line_text)
    }

    /// Returns the current token without advancing.
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    /// Advances past the current token and returns it.
    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        self.pos += 1;
        tok
    }

    /// Consumes and returns a number token, or returns an error with source context.
    fn expect_number(&mut self) -> Result<f64, String> {
        match self.peek().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(n)
            }
            ref tok => {
                let msg = format!("Expected number, got {:?}", tok);
                Err(self.error_with_context(msg))
            }
        }
    }

    /// Consumes and returns an identifier token, or returns an error with source context.
    fn expect_identifier(&mut self) -> Result<String, String> {
        match self.peek().clone() {
            Token::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            ref tok => {
                let msg = format!("Expected identifier, got {:?}", tok);
                Err(self.error_with_context(msg))
            }
        }
    }

    /// Returns true if the current token indicates the end of a statement (newline, EOF, or colon).
    fn at_statement_end(&self) -> bool {
        matches!(self.peek(), Token::Newline | Token::Eof | Token::Colon | Token::Else)
    }

    /// Delegates expression parsing to the dedicated `ExprParser`, advancing past consumed tokens.
    fn parse_expression(&mut self) -> Result<Expr, String> {
        let mut expr_parser = ExprParser::new(&self.tokens[self.pos..]);
        let result = expr_parser.parse_expression().map_err(|e| self.error_with_context(e))?;
        self.pos += expr_parser.pos();
        Ok(result)
    }

    /// Parses all lines into a `Program`, skipping blank lines and sorting by line number.
    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut lines = Vec::new();
        loop {
            // Skip blank lines
            while *self.peek() == Token::Newline {
                self.advance();
                self.source_line += 1;
            }
            if *self.peek() == Token::Eof {
                break;
            }
            lines.push(self.parse_line()?);
        }
        // Sort lines by line number
        lines.sort_by_key(|l| l.line_number);
        Ok(Program {
            lines,
            source_lines: self.source_lines.clone(),
        })
    }

    /// Parses a single BASIC line: a line number followed by one or more colon-separated statements.
    fn parse_line(&mut self) -> Result<Line, String> {
        let current_source_line = self.source_line;
        let line_number = self.expect_number()? as u32;
        let mut statements = Vec::new();
        statements.push(self.parse_statement()?);
        while *self.peek() == Token::Colon {
            self.advance();
            statements.push(self.parse_statement()?);
        }
        // Consume newline or EOF
        if *self.peek() == Token::Newline {
            self.advance();
            self.source_line += 1;
        }
        Ok(Line {
            line_number,
            statements,
            source_line: current_source_line,
        })
    }

    /// Parses a single statement by dispatching on the leading keyword token.
    /// An identifier at the start of a statement is treated as an implicit LET.
    fn parse_statement(&mut self) -> Result<Statement, String> {
        match self.peek().clone() {
            Token::Let => {
                self.advance();
                self.parse_let_body()
            }
            Token::Print => {
                self.advance();
                self.parse_print()
            }
            Token::If => {
                self.advance();
                self.parse_if()
            }
            Token::Goto => {
                self.advance();
                let line_num = self.expect_number()? as u32;
                Ok(Statement::Goto { line_number: line_num })
            }
            Token::Input => {
                self.advance();
                self.parse_input()
            }
            Token::For => {
                self.advance();
                self.parse_for()
            }
            Token::Next => {
                self.advance();
                let variable = if let Token::Identifier(_) = self.peek() {
                    Some(self.expect_identifier()?)
                } else {
                    None
                };
                Ok(Statement::Next { variable })
            }
            Token::Rem(text) => {
                self.advance();
                Ok(Statement::Rem(text))
            }
            Token::End => {
                self.advance();
                Ok(Statement::End)
            }
            Token::Def => {
                self.advance();
                self.parse_def_fn()
            }
            Token::Data => {
                self.advance();
                self.parse_data()
            }
            Token::Read => {
                self.advance();
                self.parse_read()
            }
            Token::Restore => {
                self.advance();
                self.parse_restore()
            }
            Token::Dim => {
                self.advance();
                self.parse_dim()
            }
            Token::Erase => {
                self.advance();
                self.parse_erase()
            }
            Token::On => {
                self.advance();
                self.parse_on_gosub()
            }
            Token::Gosub => {
                self.advance();
                let target = self.parse_expression()?;
                Ok(Statement::Gosub { target })
            }
            Token::Return => {
                self.advance();
                let target = if self.at_statement_end() {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                Ok(Statement::Return { target })
            }
            Token::Locate => {
                self.advance();
                self.parse_locate()
            }
            Token::Cls => {
                self.advance();
                self.parse_cls()
            }
            Token::Color => {
                self.advance();
                self.parse_color()
            }
            Token::Identifier(_) => {
                // Implicit LET: variable = expression
                self.parse_let_body()
            }
            ref tok => {
                let msg = format!("Unexpected token at start of statement: {:?}", tok);
                Err(self.error_with_context(msg))
            }
        }
    }

    /// Parses the body of a LET statement: `variable[(subscripts)] = expression`.
    /// Supports both scalar assignment (`X = 5`) and array element assignment (`A(1, 2) = 5`).
    fn parse_let_body(&mut self) -> Result<Statement, String> {
        let variable = self.expect_identifier()?;
        let indices = if *self.peek() == Token::LeftParen {
            self.advance();
            let mut idx = Vec::new();
            idx.push(self.parse_expression()?);
            while *self.peek() == Token::Comma {
                self.advance();
                idx.push(self.parse_expression()?);
            }
            if *self.peek() != Token::RightParen {
                return Err(self.error_with_context("Expected ')' after array subscripts".to_string()));
            }
            self.advance();
            idx
        } else {
            Vec::new()
        };
        if *self.peek() != Token::Equal {
            let msg = format!("Expected '=' after variable in LET, got {:?}", self.peek());
            return Err(self.error_with_context(msg));
        }
        self.advance();
        let expression = self.parse_expression()?;
        Ok(Statement::Let {
            variable,
            indices,
            expression,
        })
    }

    /// Parses a PRINT statement's item list (expressions, semicolons, and commas).
    fn parse_print(&mut self) -> Result<Statement, String> {
        let mut items = Vec::new();
        while !self.at_statement_end() {
            match self.peek() {
                Token::Semicolon => {
                    self.advance();
                    items.push(PrintItem::Semicolon);
                }
                Token::Comma => {
                    self.advance();
                    items.push(PrintItem::Comma);
                }
                _ => {
                    let expr = self.parse_expression()?;
                    items.push(PrintItem::Expression(expr));
                }
            }
        }
        Ok(Statement::Print { items })
    }

    /// Parses an IF statement: `IF condition THEN (line_number | statement) [ELSE (line_number | statement)]`.
    fn parse_if(&mut self) -> Result<Statement, String> {
        let condition = self.parse_expression()?;
        if *self.peek() != Token::Then {
            let msg = format!("Expected THEN, got {:?}", self.peek());
            return Err(self.error_with_context(msg));
        }
        self.advance();
        // THEN can be followed by a line number or a statement
        let then_clause = if let Token::Number(n) = self.peek().clone() {
            self.advance();
            ThenClause::LineNumber(n as u32)
        } else {
            ThenClause::Statement(Box::new(self.parse_statement()?))
        };
        // Optional ELSE clause
        let else_clause = if *self.peek() == Token::Else {
            self.advance();
            let clause = if let Token::Number(n) = self.peek().clone() {
                self.advance();
                ThenClause::LineNumber(n as u32)
            } else {
                ThenClause::Statement(Box::new(self.parse_statement()?))
            };
            Some(Box::new(clause))
        } else {
            None
        };
        Ok(Statement::If {
            condition,
            then: Box::new(then_clause),
            else_clause,
        })
    }

    /// Parses an INPUT statement: `INPUT ["prompt" (";" | ",")] variable[(subscripts)]`.
    ///
    /// When a semicolon separates the prompt from the variable, a "? " is appended
    /// to the prompt at runtime (GW-BASIC default). When a comma is used instead,
    /// the question mark is suppressed.
    fn parse_input(&mut self) -> Result<Statement, String> {
        // INPUT [string (";" | ",")] variable[(subscripts)]
        let prompt;
        let variable;
        let suppress_question_mark;

        match self.peek().clone() {
            Token::StringLiteral(s) => {
                self.advance();
                if *self.peek() == Token::Semicolon {
                    self.advance();
                    prompt = Some(s);
                    suppress_question_mark = false;
                    variable = self.expect_identifier()?;
                } else if *self.peek() == Token::Comma {
                    self.advance();
                    prompt = Some(s);
                    suppress_question_mark = true;
                    variable = self.expect_identifier()?;
                } else {
                    return Err(self.error_with_context("Expected ';' or ',' after INPUT prompt string".to_string()));
                }
            }
            Token::Identifier(_) => {
                prompt = None;
                suppress_question_mark = false;
                variable = self.expect_identifier()?;
            }
            ref tok => {
                let msg = format!("Expected variable or string in INPUT, got {:?}", tok);
                return Err(self.error_with_context(msg));
            }
        }

        let indices = self.parse_optional_subscripts()?;
        Ok(Statement::Input {
            prompt,
            variable,
            indices,
            suppress_question_mark,
        })
    }

    /// Parses a FOR statement: `FOR var = start TO end [STEP step]`.
    fn parse_for(&mut self) -> Result<Statement, String> {
        let variable = self.expect_identifier()?;
        if *self.peek() != Token::Equal {
            return Err(self.error_with_context("Expected '=' in FOR".to_string()));
        }
        self.advance();
        let start = self.parse_expression()?;
        if *self.peek() != Token::To {
            return Err(self.error_with_context("Expected TO in FOR".to_string()));
        }
        self.advance();
        let end = self.parse_expression()?;
        let step = if *self.peek() == Token::Step {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };
        Ok(Statement::For {
            variable,
            start,
            end,
            step,
        })
    }

    /// Parses a DEF FN statement: `DEF FN<name>[(<params>)] = <expression>`.
    /// The function name is stored as "FN<name>" to match how it will be called.
    fn parse_def_fn(&mut self) -> Result<Statement, String> {
        // Expect an identifier starting with "FN" (e.g., FNMUL or FN followed by MUL)
        let fn_name_raw = self.expect_identifier()?;
        let fn_name = if fn_name_raw == "FN" {
            // "DEF FN MUL" case - FN and name are separate tokens
            // Next could be an identifier (the name) or a paren-less function
            if let Token::Identifier(_) = self.peek() {
                let suffix = self.expect_identifier()?;
                format!("FN{}", suffix)
            } else {
                // Parameterless: DEF FN = expr (just "FN" as the name)
                "FN".to_string()
            }
        } else if fn_name_raw.starts_with("FN") {
            // "DEF FNMUL" case - already concatenated
            fn_name_raw
        } else {
            return Err(self.error_with_context(format!("Expected FN<name> after DEF, got {}", fn_name_raw)));
        };
        // Parse optional parameter list
        let mut params = Vec::new();
        if *self.peek() == Token::LeftParen {
            self.advance();
            if *self.peek() != Token::RightParen {
                params.push(self.expect_identifier()?);
                while *self.peek() == Token::Comma {
                    self.advance();
                    params.push(self.expect_identifier()?);
                }
            }
            if *self.peek() != Token::RightParen {
                return Err(self.error_with_context("Expected ')' after DEF FN parameter list".to_string()));
            }
            self.advance();
        }
        // Expect '='
        if *self.peek() != Token::Equal {
            return Err(self.error_with_context(format!("Expected '=' in DEF FN, got {:?}", self.peek())));
        }
        self.advance();
        let body = self.parse_expression()?;
        Ok(Statement::DefFn {
            name: fn_name,
            params,
            body,
        })
    }

    /// Parses a DATA statement: `DATA constant {, constant}`.
    /// Each constant is either a numeric literal, a string literal, or an unquoted string
    /// (any non-separator text treated as a string). Unquoted strings are delimited by commas,
    /// colons, or end-of-line.
    fn parse_data(&mut self) -> Result<Statement, String> {
        let mut values = Vec::new();
        loop {
            if self.at_statement_end() {
                break;
            }
            match self.peek().clone() {
                Token::Number(n) => {
                    self.advance();
                    // Check if this is a negative number: if followed by nothing special, it's just a number
                    values.push(Expr::Number(n));
                }
                Token::Minus => {
                    // Negative number in DATA
                    self.advance();
                    let n = self.expect_number()?;
                    values.push(Expr::Number(-n));
                }
                Token::StringLiteral(s) => {
                    self.advance();
                    values.push(Expr::StringLiteral(s));
                }
                _ => {
                    // Unquoted string: read identifier tokens as string constants
                    // In GW-BASIC, unquoted DATA items that aren't numbers are treated as strings
                    let name = self.expect_identifier()?;
                    values.push(Expr::StringLiteral(name));
                }
            }
            // Expect comma between values or end of statement
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(Statement::Data { values })
    }

    /// Parses a READ statement: `READ variable[(subscripts)] {, variable[(subscripts)]}`.
    fn parse_read(&mut self) -> Result<Statement, String> {
        let mut variables = Vec::new();
        let name = self.expect_identifier()?;
        let indices = self.parse_optional_subscripts()?;
        variables.push((name, indices));
        while *self.peek() == Token::Comma {
            self.advance();
            let name = self.expect_identifier()?;
            let indices = self.parse_optional_subscripts()?;
            variables.push((name, indices));
        }
        Ok(Statement::Read { variables })
    }

    /// Parses optional subscript list `(expr, expr, ...)` after a variable name.
    /// Returns an empty vector if no `(` follows.
    fn parse_optional_subscripts(&mut self) -> Result<Vec<Expr>, String> {
        if *self.peek() == Token::LeftParen {
            self.advance();
            let mut indices = Vec::new();
            indices.push(self.parse_expression()?);
            while *self.peek() == Token::Comma {
                self.advance();
                indices.push(self.parse_expression()?);
            }
            if *self.peek() != Token::RightParen {
                return Err(self.error_with_context("Expected ')' after subscripts".to_string()));
            }
            self.advance();
            Ok(indices)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses a DIM statement: `DIM variable(subscripts) [, variable(subscripts)]...`.
    fn parse_dim(&mut self) -> Result<Statement, String> {
        let mut arrays = Vec::new();
        loop {
            let name = self.expect_identifier()?;
            if *self.peek() != Token::LeftParen {
                return Err(self.error_with_context("Expected '(' after array name in DIM".to_string()));
            }
            self.advance();
            let mut dims = Vec::new();
            dims.push(self.parse_expression()?);
            while *self.peek() == Token::Comma {
                self.advance();
                dims.push(self.parse_expression()?);
            }
            if *self.peek() != Token::RightParen {
                return Err(self.error_with_context("Expected ')' in DIM".to_string()));
            }
            self.advance();
            arrays.push((name, dims));
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(Statement::Dim { arrays })
    }

    /// Parses an ERASE statement: `ERASE arrayname [, arrayname]...`.
    fn parse_erase(&mut self) -> Result<Statement, String> {
        let mut arrays = Vec::new();
        arrays.push(self.expect_identifier()?);
        while *self.peek() == Token::Comma {
            self.advance();
            arrays.push(self.expect_identifier()?);
        }
        Ok(Statement::Erase { arrays })
    }

    /// Parses a RESTORE statement: `RESTORE [line_number]`.
    fn parse_restore(&mut self) -> Result<Statement, String> {
        let line_number = if let Token::Number(n) = self.peek().clone() {
            self.advance();
            Some(n as u32)
        } else {
            None
        };
        Ok(Statement::Restore { line_number })
    }

    /// Parses an `ON expr GOSUB line1, line2, ...` statement.
    /// The selector expression is evaluated at runtime to choose which target line to GOSUB to.
    /// Targets are 1-indexed: ON 1 GOSUB 100,200 calls line 100, ON 2 calls line 200.
    fn parse_on_gosub(&mut self) -> Result<Statement, String> {
        let selector = self.parse_expression()?;
        if *self.peek() != Token::Gosub {
            return Err(self.error_with_context("Expected GOSUB after ON expression".to_string()));
        }
        self.advance(); // consume GOSUB
        let mut targets = Vec::new();
        if let Token::Number(n) = self.peek().clone() {
            self.advance();
            targets.push(n as u32);
        } else {
            return Err(self.error_with_context("Expected line number after ON...GOSUB".to_string()));
        }
        while *self.peek() == Token::Comma {
            self.advance();
            if let Token::Number(n) = self.peek().clone() {
                self.advance();
                targets.push(n as u32);
            } else {
                return Err(self.error_with_context("Expected line number after comma in ON...GOSUB".to_string()));
            }
        }
        Ok(Statement::OnGosub { selector, targets })
    }

    /// Parses a LOCATE statement: `LOCATE [row][,[col][,[cursor][,[start][,stop]]]]`.
    /// All parameters are optional and may be omitted by using consecutive commas.
    fn parse_locate(&mut self) -> Result<Statement, String> {
        let mut params: Vec<Option<Expr>> = Vec::new();
        // Parse up to 5 optional comma-separated parameters
        for i in 0..5 {
            if self.at_statement_end() {
                break;
            }
            if i > 0 {
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            // Check if this parameter is omitted (next token is comma, end of statement, or we're done)
            if self.at_statement_end() || *self.peek() == Token::Comma {
                params.push(None);
            } else {
                params.push(Some(self.parse_expression()?));
            }
        }
        Ok(Statement::Locate {
            row: params.first().cloned().flatten(),
            col: params.get(1).cloned().flatten(),
            cursor: params.get(2).cloned().flatten(),
            start: params.get(3).cloned().flatten(),
            stop: params.get(4).cloned().flatten(),
        })
    }

    /// Parses a CLS statement: `CLS [n]`.
    fn parse_cls(&mut self) -> Result<Statement, String> {
        let mode = if self.at_statement_end() {
            None
        } else {
            Some(self.parse_expression()?)
        };
        Ok(Statement::Cls { mode })
    }

    /// Parses a COLOR statement: `COLOR [foreground][,[background][,border]]`.
    /// All parameters are optional and may be omitted.
    fn parse_color(&mut self) -> Result<Statement, String> {
        let mut params: Vec<Option<Expr>> = Vec::new();
        // Parse up to 3 optional comma-separated parameters
        for i in 0..3 {
            if self.at_statement_end() {
                break;
            }
            if i > 0 {
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            if self.at_statement_end() || *self.peek() == Token::Comma {
                params.push(None);
            } else {
                params.push(Some(self.parse_expression()?));
            }
        }
        Ok(Statement::Color {
            foreground: params.first().cloned().flatten(),
            background: params.get(1).cloned().flatten(),
            border: params.get(2).cloned().flatten(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{BinOp, Expr};
    use crate::token::Lexer;

    fn parse_program(input: &str) -> Program {
        let tokens = Lexer::new(input).tokenize();
        let source_lines: Vec<String> = input.lines().map(String::from).collect();
        let mut parser = Parser::new(&tokens, source_lines);
        parser.parse_program().unwrap()
    }

    fn parse_single_statement(input: &str) -> Statement {
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 1);
        assert_eq!(prog.lines[0].statements.len(), 1);
        prog.lines[0].statements[0].clone()
    }

    #[test]
    fn test_parse_let_explicit() {
        let stmt = parse_single_statement("10 LET X = 5");
        assert_eq!(
            stmt,
            Statement::Let {
                variable: "X".to_string(),
                indices: vec![],
                expression: Expr::Number(5.0),
            }
        );
    }

    #[test]
    fn test_parse_let_implicit() {
        let stmt = parse_single_statement("10 X = 5");
        assert_eq!(
            stmt,
            Statement::Let {
                variable: "X".to_string(),
                indices: vec![],
                expression: Expr::Number(5.0),
            }
        );
    }

    #[test]
    fn test_parse_let_expression() {
        let stmt = parse_single_statement("30 C = (A + B) * 2");
        assert_eq!(
            stmt,
            Statement::Let {
                variable: "C".to_string(),
                indices: vec![],
                expression: Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::BinaryOp {
                        op: BinOp::Add,
                        left: Box::new(Expr::Variable("A".to_string())),
                        right: Box::new(Expr::Variable("B".to_string())),
                    }),
                    right: Box::new(Expr::Number(2.0)),
                },
            }
        );
    }

    #[test]
    fn test_parse_print_string() {
        let stmt = parse_single_statement("10 PRINT \"HELLO\"");
        assert_eq!(
            stmt,
            Statement::Print {
                items: vec![PrintItem::Expression(Expr::StringLiteral("HELLO".to_string()))],
            }
        );
    }

    #[test]
    fn test_parse_print_empty() {
        let stmt = parse_single_statement("90 PRINT");
        assert_eq!(stmt, Statement::Print { items: vec![] });
    }

    #[test]
    fn test_parse_print_with_semicolon() {
        let stmt = parse_single_statement("30 PRINT \"HELLO \"; N$; \"!\"");
        assert_eq!(
            stmt,
            Statement::Print {
                items: vec![
                    PrintItem::Expression(Expr::StringLiteral("HELLO ".to_string())),
                    PrintItem::Semicolon,
                    PrintItem::Expression(Expr::Variable("N$".to_string())),
                    PrintItem::Semicolon,
                    PrintItem::Expression(Expr::StringLiteral("!".to_string())),
                ],
            }
        );
    }

    #[test]
    fn test_parse_print_with_comma() {
        let stmt = parse_single_statement("10 PRINT A, B");
        assert_eq!(
            stmt,
            Statement::Print {
                items: vec![
                    PrintItem::Expression(Expr::Variable("A".to_string())),
                    PrintItem::Comma,
                    PrintItem::Expression(Expr::Variable("B".to_string())),
                ],
            }
        );
    }

    #[test]
    fn test_parse_print_trailing_semicolon() {
        let stmt = parse_single_statement("30 PRINT 2 ^ I;");
        assert_eq!(
            stmt,
            Statement::Print {
                items: vec![
                    PrintItem::Expression(Expr::BinaryOp {
                        op: BinOp::Pow,
                        left: Box::new(Expr::Number(2.0)),
                        right: Box::new(Expr::Variable("I".to_string())),
                    }),
                    PrintItem::Semicolon,
                ],
            }
        );
    }

    #[test]
    fn test_parse_if_then_statement() {
        let stmt = parse_single_statement("30 IF G = X THEN PRINT \"CORRECT!\"");
        assert_eq!(
            stmt,
            Statement::If {
                condition: Expr::BinaryOp {
                    op: BinOp::Equal,
                    left: Box::new(Expr::Variable("G".to_string())),
                    right: Box::new(Expr::Variable("X".to_string())),
                },
                then: Box::new(ThenClause::Statement(Box::new(Statement::Print {
                    items: vec![PrintItem::Expression(Expr::StringLiteral("CORRECT!".to_string()))],
                }))),
                else_clause: None,
            }
        );
    }

    #[test]
    fn test_parse_if_then_goto() {
        let stmt = parse_single_statement("50 IF X <= 3 THEN GOTO 30");
        assert_eq!(
            stmt,
            Statement::If {
                condition: Expr::BinaryOp {
                    op: BinOp::LessEqual,
                    left: Box::new(Expr::Variable("X".to_string())),
                    right: Box::new(Expr::Number(3.0)),
                },
                then: Box::new(ThenClause::Statement(Box::new(Statement::Goto { line_number: 30 }))),
                else_clause: None,
            }
        );
    }

    #[test]
    fn test_parse_if_then_line_number() {
        let stmt = parse_single_statement("80 IF G = SECRET THEN 130");
        assert_eq!(
            stmt,
            Statement::If {
                condition: Expr::BinaryOp {
                    op: BinOp::Equal,
                    left: Box::new(Expr::Variable("G".to_string())),
                    right: Box::new(Expr::Variable("SECRET".to_string())),
                },
                then: Box::new(ThenClause::LineNumber(130)),
                else_clause: None,
            }
        );
    }

    #[test]
    fn test_parse_goto() {
        let stmt = parse_single_statement("110 GOTO 50");
        assert_eq!(stmt, Statement::Goto { line_number: 50 });
    }

    #[test]
    fn test_parse_input_simple() {
        let stmt = parse_single_statement("20 INPUT N$");
        assert_eq!(
            stmt,
            Statement::Input {
                prompt: None,
                variable: "N$".to_string(),
                indices: vec![],
                suppress_question_mark: false,
            }
        );
    }

    #[test]
    fn test_parse_input_with_prompt() {
        let stmt = parse_single_statement("20 INPUT \"GUESS (1-10): \"; G");
        assert_eq!(
            stmt,
            Statement::Input {
                prompt: Some("GUESS (1-10): ".to_string()),
                variable: "G".to_string(),
                indices: vec![],
                suppress_question_mark: false,
            }
        );
    }

    #[test]
    fn test_parse_input_with_prompt_comma() {
        let stmt = parse_single_statement("20 INPUT \"ENTER VALUE: \", G");
        assert_eq!(
            stmt,
            Statement::Input {
                prompt: Some("ENTER VALUE: ".to_string()),
                variable: "G".to_string(),
                indices: vec![],
                suppress_question_mark: true,
            }
        );
    }

    #[test]
    fn test_parse_input_comma_string_variable() {
        let stmt = parse_single_statement("10 INPUT \"NAME\", N$");
        assert_eq!(
            stmt,
            Statement::Input {
                prompt: Some("NAME".to_string()),
                variable: "N$".to_string(),
                indices: vec![],
                suppress_question_mark: true,
            }
        );
    }

    #[test]
    fn test_parse_for() {
        let stmt = parse_single_statement("20 FOR I = 1 TO 5");
        assert_eq!(
            stmt,
            Statement::For {
                variable: "I".to_string(),
                start: Expr::Number(1.0),
                end: Expr::Number(5.0),
                step: None,
            }
        );
    }

    #[test]
    fn test_parse_for_with_step() {
        let stmt = parse_single_statement("20 FOR I = 2 TO 10 STEP 2");
        assert_eq!(
            stmt,
            Statement::For {
                variable: "I".to_string(),
                start: Expr::Number(2.0),
                end: Expr::Number(10.0),
                step: Some(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_parse_next() {
        let stmt = parse_single_statement("40 NEXT I");
        assert_eq!(
            stmt,
            Statement::Next {
                variable: Some("I".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_next_no_var() {
        let stmt = parse_single_statement("40 NEXT");
        assert_eq!(stmt, Statement::Next { variable: None });
    }

    #[test]
    fn test_parse_rem() {
        let stmt = parse_single_statement("10 REM THIS IS A COMMENT");
        assert_eq!(stmt, Statement::Rem("THIS IS A COMMENT".to_string()));
    }

    #[test]
    fn test_parse_end() {
        let stmt = parse_single_statement("50 END");
        assert_eq!(stmt, Statement::End);
    }

    #[test]
    fn test_parse_multi_statement_line() {
        let prog = parse_program("50 ISPRIME = 0 : GOTO 70\n");
        assert_eq!(prog.lines.len(), 1);
        assert_eq!(prog.lines[0].line_number, 50);
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert_eq!(
            prog.lines[0].statements[0],
            Statement::Let {
                variable: "ISPRIME".to_string(),
                indices: vec![],
                expression: Expr::Number(0.0),
            }
        );
        assert_eq!(prog.lines[0].statements[1], Statement::Goto { line_number: 70 });
    }

    #[test]
    fn test_parse_full_program() {
        let input = "\
10 PRINT \"WHAT IS YOUR NAME?\"
20 INPUT N$
30 PRINT \"HELLO \"; N$; \"!\"
40 END
";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 4);
        assert_eq!(prog.lines[0].line_number, 10);
        assert_eq!(prog.lines[1].line_number, 20);
        assert_eq!(prog.lines[2].line_number, 30);
        assert_eq!(prog.lines[3].line_number, 40);
    }

    #[test]
    fn test_parse_lines_sorted() {
        let input = "\
30 PRINT \"C\"
10 PRINT \"A\"
20 PRINT \"B\"
";
        let prog = parse_program(input);
        assert_eq!(prog.lines[0].line_number, 10);
        assert_eq!(prog.lines[1].line_number, 20);
        assert_eq!(prog.lines[2].line_number, 30);
    }

    #[test]
    fn test_parse_counter_program() {
        let input = "\
10 REM COUNTER PROGRAM
20 LET X = 1
30 PRINT \"NUMBER:\"; X
40 X = X + 1
50 IF X <= 3 THEN GOTO 30
60 PRINT \"PROGRAM COMPLETE.\"
70 END
";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 7);
        assert!(matches!(prog.lines[0].statements[0], Statement::Rem(_)));
        assert!(matches!(prog.lines[1].statements[0], Statement::Let { .. }));
        assert!(matches!(prog.lines[2].statements[0], Statement::Print { .. }));
        assert!(matches!(prog.lines[3].statements[0], Statement::Let { .. }));
        assert!(matches!(prog.lines[4].statements[0], Statement::If { .. }));
        assert!(matches!(prog.lines[5].statements[0], Statement::Print { .. }));
        assert!(matches!(prog.lines[6].statements[0], Statement::End));
    }

    #[test]
    fn test_parse_for_loop_program() {
        let input = "\
10 PRINT \"POWERS OF 2:\"
20 FOR I = 1 TO 5
30 PRINT 2 ^ I;
40 NEXT I
50 END
";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 5);
        assert!(matches!(prog.lines[1].statements[0], Statement::For { .. }));
        assert!(matches!(prog.lines[3].statements[0], Statement::Next { .. }));
    }

    #[test]
    fn test_parse_leading_whitespace() {
        let input = "\
    10 PRINT \"A\"
    20 END
";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 2);
        assert_eq!(prog.lines[0].line_number, 10);
        assert_eq!(prog.lines[1].line_number, 20);
    }

    #[test]
    fn test_parse_empty_lines() {
        let input = "\n10 PRINT \"A\"\n\n20 PRINT \"B\"\n\n30 END\n";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 3);
        assert_eq!(prog.lines[0].line_number, 10);
        assert_eq!(prog.lines[1].line_number, 20);
        assert_eq!(prog.lines[2].line_number, 30);
    }

    #[test]
    fn test_parse_leading_whitespace_and_empty_lines() {
        let input = "\n    10 PRINT \"A\"\n\n    20 END\n\n";
        let prog = parse_program(input);
        assert_eq!(prog.lines.len(), 2);
        assert_eq!(prog.lines[0].line_number, 10);
        assert_eq!(prog.lines[1].line_number, 20);
    }

    #[test]
    fn test_parse_if_then_implicit_let() {
        // IF condition THEN variable = expr (implicit LET after THEN)
        let stmt = parse_single_statement("50 IF N / D = INT(N / D) THEN ISPRIME = 0");
        assert!(matches!(stmt, Statement::If { .. }));
        if let Statement::If { then, .. } = stmt {
            assert!(matches!(*then, ThenClause::Statement(ref s) if matches!(**s, Statement::Let { .. })));
        }
    }

    #[test]
    fn test_parse_if_then_else_statements() {
        let stmt = parse_single_statement("30 IF G = X THEN PRINT \"YES\" ELSE PRINT \"NO\"");
        assert_eq!(
            stmt,
            Statement::If {
                condition: Expr::BinaryOp {
                    op: BinOp::Equal,
                    left: Box::new(Expr::Variable("G".to_string())),
                    right: Box::new(Expr::Variable("X".to_string())),
                },
                then: Box::new(ThenClause::Statement(Box::new(Statement::Print {
                    items: vec![PrintItem::Expression(Expr::StringLiteral("YES".to_string()))],
                }))),
                else_clause: Some(Box::new(ThenClause::Statement(Box::new(Statement::Print {
                    items: vec![PrintItem::Expression(Expr::StringLiteral("NO".to_string()))],
                })))),
            }
        );
    }

    #[test]
    fn test_parse_if_then_else_line_numbers() {
        let stmt = parse_single_statement("30 IF X > 0 THEN 100 ELSE 200");
        assert_eq!(
            stmt,
            Statement::If {
                condition: Expr::BinaryOp {
                    op: BinOp::Greater,
                    left: Box::new(Expr::Variable("X".to_string())),
                    right: Box::new(Expr::Number(0.0)),
                },
                then: Box::new(ThenClause::LineNumber(100)),
                else_clause: Some(Box::new(ThenClause::LineNumber(200))),
            }
        );
    }

    #[test]
    fn test_parse_if_then_stmt_else_goto() {
        let stmt = parse_single_statement("30 IF X = 1 THEN PRINT \"ONE\" ELSE GOTO 100");
        assert!(matches!(
            stmt,
            Statement::If {
                else_clause: Some(_),
                ..
            }
        ));
        if let Statement::If { else_clause, .. } = stmt {
            assert!(matches!(
                *else_clause.unwrap(),
                ThenClause::Statement(ref s) if matches!(**s, Statement::Goto { line_number: 100 })
            ));
        }
    }

    fn parse_program_err(input: &str) -> String {
        let tokens = Lexer::new(input).tokenize();
        let source_lines: Vec<String> = input.lines().map(String::from).collect();
        let mut parser = Parser::new(&tokens, source_lines);
        parser.parse_program().unwrap_err()
    }

    #[test]
    fn test_parse_error_expected_number_for_line() {
        let err = parse_program_err("PRINT \"HI\"");
        assert!(err.contains("Expected number"));
    }

    #[test]
    fn test_parse_error_unexpected_token_at_statement() {
        let err = parse_program_err("10 +");
        assert!(err.contains("Unexpected token at start of statement"));
    }

    #[test]
    fn test_parse_error_missing_equal_in_let() {
        let err = parse_program_err("10 LET X 5");
        assert!(err.contains("Expected '=' after variable in LET"));
    }

    #[test]
    fn test_parse_error_missing_then() {
        let err = parse_program_err("10 IF X = 5 GOTO 20");
        assert!(err.contains("Expected THEN"));
    }

    #[test]
    fn test_parse_error_input_missing_separator() {
        let err = parse_program_err("10 INPUT \"PROMPT\" X");
        assert!(err.contains("Expected ';' or ',' after INPUT prompt string"));
    }

    #[test]
    fn test_parse_error_input_bad_token() {
        let err = parse_program_err("10 INPUT 42");
        assert!(err.contains("Expected variable or string in INPUT"));
    }

    #[test]
    fn test_parse_error_for_missing_equal() {
        let err = parse_program_err("10 FOR I 1 TO 5");
        assert!(err.contains("Expected '=' in FOR"));
    }

    #[test]
    fn test_parse_error_for_missing_to() {
        let err = parse_program_err("10 FOR I = 1 STEP 5");
        assert!(err.contains("Expected TO in FOR"));
    }

    #[test]
    fn test_parse_error_expected_identifier() {
        let err = parse_program_err("10 LET 42 = 5");
        assert!(err.contains("Expected identifier"));
    }

    #[test]
    fn test_parse_def_fn_with_params() {
        let stmt = parse_single_statement("10 DEF FNMUL(A, B) = A * B");
        assert!(matches!(stmt, Statement::DefFn { .. }));
        if let Statement::DefFn { name, params, body } = stmt {
            assert_eq!(name, "FNMUL");
            assert_eq!(params, vec!["A", "B"]);
            assert_eq!(
                body,
                Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Variable("A".to_string())),
                    right: Box::new(Expr::Variable("B".to_string())),
                }
            );
        }
    }

    #[test]
    fn test_parse_def_fn_no_params() {
        let stmt = parse_single_statement("10 DEF FNPI = 3.14");
        assert!(matches!(stmt, Statement::DefFn { .. }));
        if let Statement::DefFn { name, params, .. } = stmt {
            assert_eq!(name, "FNPI");
            assert!(params.is_empty());
        }
    }

    #[test]
    fn test_parse_def_fn_space_separated() {
        let stmt = parse_single_statement("10 DEF FN MUL(A, B) = A * B");
        assert!(matches!(stmt, Statement::DefFn { .. }));
        if let Statement::DefFn { name, params, .. } = stmt {
            assert_eq!(name, "FNMUL");
            assert_eq!(params, vec!["A", "B"]);
        }
    }

    #[test]
    fn test_parse_def_fn_error_not_fn() {
        let err = parse_program_err("10 DEF MUL(A) = A");
        assert!(err.contains("Expected FN<name> after DEF"));
    }

    #[test]
    fn test_parse_def_fn_error_missing_equal() {
        let err = parse_program_err("10 DEF FNMUL(A, B) A * B");
        assert!(err.contains("Expected '=' in DEF FN"));
    }

    #[test]
    fn test_parse_def_fn_string_function() {
        let stmt = parse_single_statement("10 DEF FNGET$(X$) = LEFT$(X$, 1)");
        assert!(matches!(stmt, Statement::DefFn { .. }));
        if let Statement::DefFn { name, params, .. } = stmt {
            assert_eq!(name, "FNGET$");
            assert_eq!(params, vec!["X$"]);
        }
    }

    #[test]
    fn test_parse_def_fn_error_missing_rparen() {
        let err = parse_program_err("10 DEF FNMUL(A, B = A * B");
        assert!(err.contains("Expected ')' after DEF FN parameter list"));
    }

    #[test]
    fn test_parse_def_fn_bare_fn() {
        // DEF FN = expr (bare FN, no suffix name)
        let stmt = parse_single_statement("10 DEF FN = 42");
        assert!(matches!(stmt, Statement::DefFn { .. }));
        if let Statement::DefFn { name, params, .. } = stmt {
            assert_eq!(name, "FN");
            assert!(params.is_empty());
        }
    }

    // --- DATA / READ / RESTORE parser tests ---

    #[test]
    fn test_parse_data_numbers() {
        let stmt = parse_single_statement("10 DATA 1, 2, 3");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![Expr::Number(1.0), Expr::Number(2.0), Expr::Number(3.0)],
            }
        );
    }

    #[test]
    fn test_parse_data_strings() {
        let stmt = parse_single_statement("10 DATA \"HELLO\", \"WORLD\"");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![
                    Expr::StringLiteral("HELLO".to_string()),
                    Expr::StringLiteral("WORLD".to_string()),
                ],
            }
        );
    }

    #[test]
    fn test_parse_data_mixed() {
        let stmt = parse_single_statement("10 DATA \"ALICE\", 25");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![Expr::StringLiteral("ALICE".to_string()), Expr::Number(25.0)],
            }
        );
    }

    #[test]
    fn test_parse_data_negative_number() {
        let stmt = parse_single_statement("10 DATA -5, 10");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![Expr::Number(-5.0), Expr::Number(10.0)],
            }
        );
    }

    #[test]
    fn test_parse_data_unquoted_string() {
        let stmt = parse_single_statement("10 DATA HELLO, WORLD");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![
                    Expr::StringLiteral("HELLO".to_string()),
                    Expr::StringLiteral("WORLD".to_string()),
                ],
            }
        );
    }

    #[test]
    fn test_parse_data_single_value() {
        let stmt = parse_single_statement("10 DATA 42");
        assert_eq!(
            stmt,
            Statement::Data {
                values: vec![Expr::Number(42.0)],
            }
        );
    }

    #[test]
    fn test_parse_read_single() {
        let stmt = parse_single_statement("10 READ A");
        assert_eq!(
            stmt,
            Statement::Read {
                variables: vec![("A".to_string(), vec![])],
            }
        );
    }

    #[test]
    fn test_parse_read_multiple() {
        let stmt = parse_single_statement("10 READ A, B$, C");
        assert_eq!(
            stmt,
            Statement::Read {
                variables: vec![
                    ("A".to_string(), vec![]),
                    ("B$".to_string(), vec![]),
                    ("C".to_string(), vec![]),
                ],
            }
        );
    }

    #[test]
    fn test_parse_restore_no_line() {
        let stmt = parse_single_statement("10 RESTORE");
        assert_eq!(stmt, Statement::Restore { line_number: None });
    }

    #[test]
    fn test_parse_restore_with_line() {
        let stmt = parse_single_statement("10 RESTORE 100");
        assert_eq!(stmt, Statement::Restore { line_number: Some(100) });
    }

    #[test]
    fn test_parse_data_on_multi_statement_line() {
        let prog = parse_program("10 X = 5 : DATA 10, 20\n");
        assert_eq!(prog.lines.len(), 1);
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert!(matches!(prog.lines[0].statements[0], Statement::Let { .. }));
        assert!(matches!(prog.lines[0].statements[1], Statement::Data { .. }));
    }

    // --- DIM / ERASE / Array parser tests ---

    #[test]
    fn test_parse_dim_single_dimension() {
        let stmt = parse_single_statement("10 DIM A(10)");
        assert_eq!(
            stmt,
            Statement::Dim {
                arrays: vec![("A".to_string(), vec![Expr::Number(10.0)])],
            }
        );
    }

    #[test]
    fn test_parse_dim_multi_dimension() {
        let stmt = parse_single_statement("10 DIM B(3, 4)");
        assert_eq!(
            stmt,
            Statement::Dim {
                arrays: vec![("B".to_string(), vec![Expr::Number(3.0), Expr::Number(4.0)])],
            }
        );
    }

    #[test]
    fn test_parse_dim_multiple_arrays() {
        let stmt = parse_single_statement("10 DIM A(5), B(3, 4)");
        assert_eq!(
            stmt,
            Statement::Dim {
                arrays: vec![
                    ("A".to_string(), vec![Expr::Number(5.0)]),
                    ("B".to_string(), vec![Expr::Number(3.0), Expr::Number(4.0)]),
                ],
            }
        );
    }

    #[test]
    fn test_parse_dim_string_array() {
        let stmt = parse_single_statement("10 DIM N$(20)");
        assert_eq!(
            stmt,
            Statement::Dim {
                arrays: vec![("N$".to_string(), vec![Expr::Number(20.0)])],
            }
        );
    }

    #[test]
    fn test_parse_dim_expression_subscript() {
        let stmt = parse_single_statement("10 DIM A(N + 1)");
        assert!(matches!(stmt, Statement::Dim { .. }));
        if let Statement::Dim { arrays } = stmt {
            assert_eq!(arrays.len(), 1);
            assert_eq!(arrays[0].0, "A");
            assert!(matches!(arrays[0].1[0], Expr::BinaryOp { .. }));
        }
    }

    #[test]
    fn test_parse_erase_single() {
        let stmt = parse_single_statement("10 ERASE A");
        assert_eq!(
            stmt,
            Statement::Erase {
                arrays: vec!["A".to_string()],
            }
        );
    }

    #[test]
    fn test_parse_erase_multiple() {
        let stmt = parse_single_statement("10 ERASE A, B, C$");
        assert_eq!(
            stmt,
            Statement::Erase {
                arrays: vec!["A".to_string(), "B".to_string(), "C$".to_string()],
            }
        );
    }

    #[test]
    fn test_parse_let_array_element() {
        let stmt = parse_single_statement("10 A(1) = 5");
        assert_eq!(
            stmt,
            Statement::Let {
                variable: "A".to_string(),
                indices: vec![Expr::Number(1.0)],
                expression: Expr::Number(5.0),
            }
        );
    }

    #[test]
    fn test_parse_let_array_multi_index() {
        let stmt = parse_single_statement("10 LET B(2, 3) = 42");
        assert_eq!(
            stmt,
            Statement::Let {
                variable: "B".to_string(),
                indices: vec![Expr::Number(2.0), Expr::Number(3.0)],
                expression: Expr::Number(42.0),
            }
        );
    }

    #[test]
    fn test_parse_read_array_element() {
        let stmt = parse_single_statement("10 READ A(1), B");
        assert_eq!(
            stmt,
            Statement::Read {
                variables: vec![("A".to_string(), vec![Expr::Number(1.0)]), ("B".to_string(), vec![]),],
            }
        );
    }

    #[test]
    fn test_parse_dim_error_missing_paren() {
        let err = parse_program_err("10 DIM A");
        assert!(err.contains("Expected '(' after array name in DIM"));
    }

    #[test]
    fn test_parse_dim_error_missing_rparen() {
        let err = parse_program_err("10 DIM A(10");
        assert!(err.contains("Expected ')' in DIM"));
    }

    #[test]
    fn test_parse_gosub() {
        let stmt = parse_single_statement("10 GOSUB 100");
        assert!(matches!(stmt, Statement::Gosub { .. }));
        if let Statement::Gosub { target } = stmt {
            assert_eq!(target, Expr::Number(100.0));
        }
    }

    #[test]
    fn test_parse_gosub_expression() {
        let stmt = parse_single_statement("10 GOSUB 100 + 50");
        assert!(matches!(stmt, Statement::Gosub { .. }));
    }

    #[test]
    fn test_parse_return() {
        let stmt = parse_single_statement("10 RETURN");
        assert!(matches!(stmt, Statement::Return { target: None }));
    }

    #[test]
    fn test_parse_return_with_line_number() {
        let stmt = parse_single_statement("10 RETURN 500");
        assert!(matches!(stmt, Statement::Return { target: Some(_) }));
        if let Statement::Return { target: Some(expr) } = stmt {
            assert_eq!(expr, Expr::Number(500.0));
        }
    }

    #[test]
    fn test_parse_return_with_expression() {
        let stmt = parse_single_statement("10 RETURN 100 + 200");
        assert!(matches!(stmt, Statement::Return { target: Some(_) }));
    }

    #[test]
    fn test_parse_return_on_multi_statement_line() {
        let prog = parse_program("10 RETURN : PRINT \"AFTER\"");
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert!(matches!(
            prog.lines[0].statements[0],
            Statement::Return { target: None }
        ));
    }

    #[test]
    fn test_parse_gosub_return_in_program() {
        let prog = parse_program("10 GOSUB 100\n20 END\n100 PRINT \"HI\"\n110 RETURN");
        assert_eq!(prog.lines.len(), 4);
        assert!(matches!(prog.lines[0].statements[0], Statement::Gosub { .. }));
        assert!(matches!(
            prog.lines[3].statements[0],
            Statement::Return { target: None }
        ));
    }

    #[test]
    fn test_parse_gosub_in_if_then() {
        let stmt = parse_single_statement("10 IF X = 1 THEN GOSUB 100");
        assert!(matches!(stmt, Statement::If { .. }));
        if let Statement::If { then, .. } = stmt {
            assert!(matches!(*then, ThenClause::Statement(ref s) if matches!(**s, Statement::Gosub { .. })));
        }
    }

    #[test]
    fn test_parse_gosub_on_multi_statement_line() {
        let prog = parse_program("10 GOSUB 100 : PRINT \"AFTER\"");
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert!(matches!(prog.lines[0].statements[0], Statement::Gosub { .. }));
        assert!(matches!(prog.lines[0].statements[1], Statement::Print { .. }));
    }

    #[test]
    fn test_parse_on_gosub_basic() {
        let stmt = parse_single_statement("10 ON X GOSUB 100, 200, 300");
        match stmt {
            Statement::OnGosub { selector, targets } => {
                assert!(matches!(selector, Expr::Variable(_)));
                assert_eq!(targets, vec![100, 200, 300]);
            }
            _ => panic!("Expected OnGosub, got {:?}", stmt),
        }
    }

    #[test]
    fn test_parse_on_gosub_single_target() {
        let stmt = parse_single_statement("10 ON I GOSUB 500");
        match stmt {
            Statement::OnGosub { targets, .. } => {
                assert_eq!(targets, vec![500]);
            }
            _ => panic!("Expected OnGosub"),
        }
    }

    #[test]
    fn test_parse_on_gosub_expression_selector() {
        let stmt = parse_single_statement("10 ON A + 1 GOSUB 100, 200");
        match stmt {
            Statement::OnGosub { selector, targets } => {
                assert!(matches!(selector, Expr::BinaryOp { .. }));
                assert_eq!(targets, vec![100, 200]);
            }
            _ => panic!("Expected OnGosub"),
        }
    }

    #[test]
    fn test_parse_on_gosub_on_multi_statement_line() {
        let prog = parse_program("10 ON I GOSUB 100, 200 : PRINT \"AFTER\"");
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert!(matches!(prog.lines[0].statements[0], Statement::OnGosub { .. }));
        assert!(matches!(prog.lines[0].statements[1], Statement::Print { .. }));
    }

    #[test]
    fn test_parse_on_gosub_error_missing_gosub() {
        let tokens = Lexer::new("10 ON X 100, 200").tokenize();
        let source_lines = vec!["10 ON X 100, 200".to_string()];
        let mut parser = Parser::new(&tokens, source_lines);
        let result = parser.parse_program();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected GOSUB"));
    }

    #[test]
    fn test_parse_on_gosub_error_missing_line_number() {
        let tokens = Lexer::new("10 ON X GOSUB").tokenize();
        let source_lines = vec!["10 ON X GOSUB".to_string()];
        let mut parser = Parser::new(&tokens, source_lines);
        let result = parser.parse_program();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected line number"));
    }

    #[test]
    fn test_parse_on_gosub_in_program() {
        let prog = parse_program(
            "\
10 ON I GOSUB 100, 200
20 END
100 PRINT \"FIRST\"
110 RETURN
200 PRINT \"SECOND\"
210 RETURN
",
        );
        assert_eq!(prog.lines.len(), 6);
        assert!(matches!(prog.lines[0].statements[0], Statement::OnGosub { .. }));
    }

    #[test]
    fn test_parse_cls_no_args() {
        let stmt = parse_single_statement("10 CLS");
        assert_eq!(stmt, Statement::Cls { mode: None });
    }

    #[test]
    fn test_parse_cls_with_arg() {
        let stmt = parse_single_statement("10 CLS 2");
        assert_eq!(
            stmt,
            Statement::Cls {
                mode: Some(Expr::Number(2.0))
            }
        );
    }

    #[test]
    fn test_parse_locate_row_col() {
        let stmt = parse_single_statement("10 LOCATE 1, 1");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: Some(Expr::Number(1.0)),
                col: Some(Expr::Number(1.0)),
                cursor: None,
                start: None,
                stop: None,
            }
        );
    }

    #[test]
    fn test_parse_locate_all_params() {
        let stmt = parse_single_statement("10 LOCATE 5, 1, 1, 0, 7");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: Some(Expr::Number(5.0)),
                col: Some(Expr::Number(1.0)),
                cursor: Some(Expr::Number(1.0)),
                start: Some(Expr::Number(0.0)),
                stop: Some(Expr::Number(7.0)),
            }
        );
    }

    #[test]
    fn test_parse_locate_omitted_params() {
        let stmt = parse_single_statement("10 LOCATE ,,1");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: None,
                col: None,
                cursor: Some(Expr::Number(1.0)),
                start: None,
                stop: None,
            }
        );
    }

    #[test]
    fn test_parse_locate_row_only() {
        let stmt = parse_single_statement("10 LOCATE 10");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: Some(Expr::Number(10.0)),
                col: None,
                cursor: None,
                start: None,
                stop: None,
            }
        );
    }

    #[test]
    fn test_parse_locate_no_args() {
        let stmt = parse_single_statement("10 LOCATE");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: None,
                col: None,
                cursor: None,
                start: None,
                stop: None,
            }
        );
    }

    #[test]
    fn test_parse_color_all_params() {
        let stmt = parse_single_statement("10 COLOR 7, 0, 3");
        assert_eq!(
            stmt,
            Statement::Color {
                foreground: Some(Expr::Number(7.0)),
                background: Some(Expr::Number(0.0)),
                border: Some(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_color_fg_only() {
        let stmt = parse_single_statement("10 COLOR 14");
        assert_eq!(
            stmt,
            Statement::Color {
                foreground: Some(Expr::Number(14.0)),
                background: None,
                border: None,
            }
        );
    }

    #[test]
    fn test_parse_color_no_args() {
        let stmt = parse_single_statement("10 COLOR");
        assert_eq!(
            stmt,
            Statement::Color {
                foreground: None,
                background: None,
                border: None,
            }
        );
    }

    #[test]
    fn test_parse_color_omitted_fg() {
        let stmt = parse_single_statement("10 COLOR ,2");
        assert_eq!(
            stmt,
            Statement::Color {
                foreground: None,
                background: Some(Expr::Number(2.0)),
                border: None,
            }
        );
    }

    #[test]
    fn test_parse_locate_with_expressions() {
        let stmt = parse_single_statement("10 LOCATE R, C");
        assert_eq!(
            stmt,
            Statement::Locate {
                row: Some(Expr::Variable("R".to_string())),
                col: Some(Expr::Variable("C".to_string())),
                cursor: None,
                start: None,
                stop: None,
            }
        );
    }

    #[test]
    fn test_parse_cls_on_multi_statement_line() {
        let prog = parse_program("10 CLS : PRINT \"HELLO\"");
        assert_eq!(prog.lines[0].statements.len(), 2);
        assert!(matches!(prog.lines[0].statements[0], Statement::Cls { mode: None }));
    }

    #[test]
    fn test_parse_color_fg_bg() {
        let stmt = parse_single_statement("10 COLOR 1, 2");
        assert_eq!(
            stmt,
            Statement::Color {
                foreground: Some(Expr::Number(1.0)),
                background: Some(Expr::Number(2.0)),
                border: None,
            }
        );
    }
}
