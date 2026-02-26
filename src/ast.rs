use crate::expr::{Expr, ExprParser};
use crate::token::Token;

/// AST node for a BASIC statement
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Let {
        variable: String,
        expression: Expr,
    },
    Print {
        items: Vec<PrintItem>,
    },
    If {
        condition: Expr,
        then: Box<ThenClause>,
    },
    Goto {
        line_number: u32,
    },
    Input {
        prompt: Option<String>,
        variable: String,
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
    Rem(String),
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrintItem {
    Expression(Expr),
    Semicolon,
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThenClause {
    LineNumber(u32),
    Statement(Statement),
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

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        self.pos += 1;
        tok
    }

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

    fn at_statement_end(&self) -> bool {
        matches!(self.peek(), Token::Newline | Token::Eof | Token::Colon)
    }

    fn parse_expression(&mut self) -> Result<Expr, String> {
        let mut expr_parser = ExprParser::new(&self.tokens[self.pos..]);
        let result = expr_parser.parse_expression().map_err(|e| self.error_with_context(e))?;
        self.pos += expr_parser.pos();
        Ok(result)
    }

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
                let text = text;
                self.advance();
                Ok(Statement::Rem(text))
            }
            Token::End => {
                self.advance();
                Ok(Statement::End)
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

    fn parse_let_body(&mut self) -> Result<Statement, String> {
        let variable = self.expect_identifier()?;
        if *self.peek() != Token::Equal {
            let msg = format!("Expected '=' after variable in LET, got {:?}", self.peek());
            return Err(self.error_with_context(msg));
        }
        self.advance();
        let expression = self.parse_expression()?;
        Ok(Statement::Let { variable, expression })
    }

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
            ThenClause::Statement(self.parse_statement()?)
        };
        Ok(Statement::If {
            condition,
            then: Box::new(then_clause),
        })
    }

    fn parse_input(&mut self) -> Result<Statement, String> {
        // INPUT [string ";"] variable
        let prompt;
        let variable;

        match self.peek().clone() {
            Token::StringLiteral(s) => {
                self.advance();
                if *self.peek() == Token::Semicolon {
                    self.advance();
                    prompt = Some(s);
                    variable = self.expect_identifier()?;
                } else {
                    // This shouldn't happen in valid BASIC, but handle gracefully
                    return Err(self.error_with_context("Expected ';' after INPUT prompt string".to_string()));
                }
            }
            Token::Identifier(_) => {
                prompt = None;
                variable = self.expect_identifier()?;
            }
            ref tok => {
                let msg = format!("Expected variable or string in INPUT, got {:?}", tok);
                return Err(self.error_with_context(msg));
            }
        }

        Ok(Statement::Input { prompt, variable })
    }

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
                then: Box::new(ThenClause::Statement(Statement::Print {
                    items: vec![PrintItem::Expression(Expr::StringLiteral("CORRECT!".to_string()))],
                })),
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
                then: Box::new(ThenClause::Statement(Statement::Goto { line_number: 30 })),
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
            assert!(matches!(*then, ThenClause::Statement(Statement::Let { .. })));
        }
    }
}
