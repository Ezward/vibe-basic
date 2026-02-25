use crate::token::Token;

/// Expression parse tree node
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    StringLiteral(String),
    Variable(String),
    BinaryOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryMinus(Box<Expr>),
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}

/// Recursive descent expression parser
pub struct ExprParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        ExprParser { tokens, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        self.pos += 1;
        tok
    }

    /// Parse a full expression (lowest precedence: comparisons)
    pub fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_comparison()
    }

    /// comparison = additive { ( "=" | "<>" | "<" | ">" | "<=" | ">=" ) additive }
    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::Equal => BinOp::Equal,
                Token::NotEqual => BinOp::NotEqual,
                Token::Less => BinOp::Less,
                Token::Greater => BinOp::Greater,
                Token::LessEqual => BinOp::LessEqual,
                Token::GreaterEqual => BinOp::GreaterEqual,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// additive = term { ( "+" | "-" ) term }
    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_term()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// term = power { ( "*" | "/" ) power }
    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// power = unary { "^" unary }
    fn parse_power(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        while *self.peek() == Token::Caret {
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp {
                op: BinOp::Pow,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// unary = "-" unary | factor
    fn parse_unary(&mut self) -> Result<Expr, String> {
        if *self.peek() == Token::Minus {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::UnaryMinus(Box::new(expr)));
        }
        self.parse_factor()
    }

    /// factor = number | string | "(" expression ")" | function_call | variable
    fn parse_factor(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Number(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::StringLiteral(s) => {
                let s = s;
                self.advance();
                Ok(Expr::StringLiteral(s))
            }
            Token::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;
                if *self.peek() != Token::RightParen {
                    return Err("Expected ')'".to_string());
                }
                self.advance();
                Ok(expr)
            }
            Token::Identifier(name) => {
                let name = name;
                self.advance();
                // Check if it's a function call
                if *self.peek() == Token::LeftParen {
                    self.advance();
                    let mut args = Vec::new();
                    if *self.peek() != Token::RightParen {
                        args.push(self.parse_expression()?);
                        while *self.peek() == Token::Comma {
                            self.advance();
                            args.push(self.parse_expression()?);
                        }
                    }
                    if *self.peek() != Token::RightParen {
                        return Err("Expected ')' after function arguments".to_string());
                    }
                    self.advance();
                    Ok(Expr::FunctionCall { name, args })
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            tok => Err(format!("Unexpected token in expression: {:?}", tok)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Lexer;

    fn parse_expr(input: &str) -> Expr {
        let tokens = Lexer::new(input).tokenize();
        let mut parser = ExprParser::new(&tokens);
        parser.parse_expression().unwrap()
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_expr("42"), Expr::Number(42.0));
    }

    #[test]
    fn test_parse_float() {
        assert_eq!(parse_expr("3.14"), Expr::Number(3.14));
    }

    #[test]
    fn test_parse_string_literal() {
        assert_eq!(parse_expr("\"HELLO\""), Expr::StringLiteral("HELLO".to_string()));
    }

    #[test]
    fn test_parse_variable() {
        assert_eq!(parse_expr("X"), Expr::Variable("X".to_string()));
    }

    #[test]
    fn test_parse_variable_with_sigil() {
        assert_eq!(parse_expr("N$"), Expr::Variable("N$".to_string()));
    }

    #[test]
    fn test_parse_addition() {
        assert_eq!(
            parse_expr("1 + 2"),
            Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_parse_subtraction() {
        assert_eq!(
            parse_expr("5 - 3"),
            Expr::BinaryOp {
                op: BinOp::Sub,
                left: Box::new(Expr::Number(5.0)),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_multiplication() {
        assert_eq!(
            parse_expr("4 * 3"),
            Expr::BinaryOp {
                op: BinOp::Mul,
                left: Box::new(Expr::Number(4.0)),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_division() {
        assert_eq!(
            parse_expr("10 / 2"),
            Expr::BinaryOp {
                op: BinOp::Div,
                left: Box::new(Expr::Number(10.0)),
                right: Box::new(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_parse_power() {
        assert_eq!(
            parse_expr("2 ^ 3"),
            Expr::BinaryOp {
                op: BinOp::Pow,
                left: Box::new(Expr::Number(2.0)),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_precedence_add_mul() {
        // 1 + 2 * 3 should be 1 + (2 * 3)
        assert_eq!(
            parse_expr("1 + 2 * 3"),
            Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Number(2.0)),
                    right: Box::new(Expr::Number(3.0)),
                }),
            }
        );
    }

    #[test]
    fn test_parse_precedence_mul_add() {
        // 2 * 3 + 1 should be (2 * 3) + 1
        assert_eq!(
            parse_expr("2 * 3 + 1"),
            Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Number(2.0)),
                    right: Box::new(Expr::Number(3.0)),
                }),
                right: Box::new(Expr::Number(1.0)),
            }
        );
    }

    #[test]
    fn test_parse_parentheses() {
        // (1 + 2) * 3
        assert_eq!(
            parse_expr("(1 + 2) * 3"),
            Expr::BinaryOp {
                op: BinOp::Mul,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Number(1.0)),
                    right: Box::new(Expr::Number(2.0)),
                }),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_nested_parens() {
        // ((1 + 2))
        assert_eq!(
            parse_expr("((1 + 2))"),
            Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::Number(1.0)),
                right: Box::new(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_parse_unary_minus() {
        assert_eq!(parse_expr("-5"), Expr::UnaryMinus(Box::new(Expr::Number(5.0))));
    }

    #[test]
    fn test_parse_unary_minus_in_expr() {
        // -X + 3
        assert_eq!(
            parse_expr("-X + 3"),
            Expr::BinaryOp {
                op: BinOp::Add,
                left: Box::new(Expr::UnaryMinus(Box::new(Expr::Variable("X".to_string())))),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_comparison_equal() {
        assert_eq!(
            parse_expr("X = 5"),
            Expr::BinaryOp {
                op: BinOp::Equal,
                left: Box::new(Expr::Variable("X".to_string())),
                right: Box::new(Expr::Number(5.0)),
            }
        );
    }

    #[test]
    fn test_parse_comparison_not_equal() {
        assert_eq!(
            parse_expr("G <> X"),
            Expr::BinaryOp {
                op: BinOp::NotEqual,
                left: Box::new(Expr::Variable("G".to_string())),
                right: Box::new(Expr::Variable("X".to_string())),
            }
        );
    }

    #[test]
    fn test_parse_comparison_less_equal() {
        assert_eq!(
            parse_expr("X <= 3"),
            Expr::BinaryOp {
                op: BinOp::LessEqual,
                left: Box::new(Expr::Variable("X".to_string())),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_function_call() {
        assert_eq!(
            parse_expr("INT(3.7)"),
            Expr::FunctionCall {
                name: "INT".to_string(),
                args: vec![Expr::Number(3.7)],
            }
        );
    }

    #[test]
    fn test_parse_function_call_multiple_args() {
        // Not standard BASIC but test the parser can handle it
        let tokens = Lexer::new("FN(1, 2)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        assert_eq!(
            expr,
            Expr::FunctionCall {
                name: "FN".to_string(),
                args: vec![Expr::Number(1.0), Expr::Number(2.0)],
            }
        );
    }

    #[test]
    fn test_parse_nested_function() {
        // INT(RND(1) * 100)
        assert_eq!(
            parse_expr("INT(RND(1) * 100)"),
            Expr::FunctionCall {
                name: "INT".to_string(),
                args: vec![Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::FunctionCall {
                        name: "RND".to_string(),
                        args: vec![Expr::Number(1.0)],
                    }),
                    right: Box::new(Expr::Number(100.0)),
                }],
            }
        );
    }

    #[test]
    fn test_parse_complex_expression() {
        // (A + B) * 2
        assert_eq!(
            parse_expr("(A + B) * 2"),
            Expr::BinaryOp {
                op: BinOp::Mul,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Variable("A".to_string())),
                    right: Box::new(Expr::Variable("B".to_string())),
                }),
                right: Box::new(Expr::Number(2.0)),
            }
        );
    }

    #[test]
    fn test_parse_left_associativity() {
        // 1 - 2 - 3 should be (1 - 2) - 3
        assert_eq!(
            parse_expr("1 - 2 - 3"),
            Expr::BinaryOp {
                op: BinOp::Sub,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Sub,
                    left: Box::new(Expr::Number(1.0)),
                    right: Box::new(Expr::Number(2.0)),
                }),
                right: Box::new(Expr::Number(3.0)),
            }
        );
    }

    #[test]
    fn test_parse_power_precedence() {
        // 2 ^ 3 * 4 should be (2^3) * 4
        assert_eq!(
            parse_expr("2 ^ 3 * 4"),
            Expr::BinaryOp {
                op: BinOp::Mul,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Pow,
                    left: Box::new(Expr::Number(2.0)),
                    right: Box::new(Expr::Number(3.0)),
                }),
                right: Box::new(Expr::Number(4.0)),
            }
        );
    }

    #[test]
    fn test_parse_comparison_with_arithmetic() {
        // X + 1 <= 3 * Y should be (X+1) <= (3*Y)
        assert_eq!(
            parse_expr("X + 1 <= 3 * Y"),
            Expr::BinaryOp {
                op: BinOp::LessEqual,
                left: Box::new(Expr::BinaryOp {
                    op: BinOp::Add,
                    left: Box::new(Expr::Variable("X".to_string())),
                    right: Box::new(Expr::Number(1.0)),
                }),
                right: Box::new(Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: Box::new(Expr::Number(3.0)),
                    right: Box::new(Expr::Variable("Y".to_string())),
                }),
            }
        );
    }
}
