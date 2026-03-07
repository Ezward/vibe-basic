//! Lexical analysis (tokenization) for BASIC source code.
//!
//! This module provides the `Token` enum representing all lexical tokens in the
//! BASIC language, and the `Lexer` struct which converts raw source text into a
//! sequence of tokens. The lexer handles case-insensitive keywords, BASIC variable
//! type sigils ($, %, !, #), numeric and string literals, operators, and comments
//! (both REM keyword and apostrophe syntax).

/// Represents a single lexical token in the BASIC language.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Number(f64),
    StringLiteral(String),
    // Identifiers / variables
    Identifier(String),
    // Keywords
    Let,
    Print,
    If,
    Then,
    Else,
    Goto,
    Input,
    For,
    To,
    Step,
    Next,
    Rem(String),
    End,
    Def,
    // Logical operators
    And,
    Or,
    Xor,
    Not,
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    // Delimiters
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Colon,
    // Special
    Newline,
    Eof,
}

/// Lexical analyzer that converts BASIC source text into a stream of tokens.
///
/// The lexer processes the input character by character, recognizing keywords,
/// identifiers, numeric literals, string literals, operators, and comments.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    /// Creates a new lexer from the given source text.
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    /// Returns the current character without advancing the position.
    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Advances the position by one and returns the character that was at the old position.
    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        self.pos += 1;
        ch
    }

    /// Skips horizontal whitespace (spaces, tabs, carriage returns) but not newlines.
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Reads a numeric literal (integer or floating-point) starting at the current position.
    fn read_number(&mut self) -> Token {
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') {
            s.push('.');
            self.advance();
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    s.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        Token::Number(s.parse::<f64>().unwrap())
    }

    /// Reads a double-quoted string literal, consuming the opening and closing quotes.
    fn read_string(&mut self) -> Token {
        self.advance(); // skip opening quote
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.advance();
                break;
            }
            s.push(ch);
            self.advance();
        }
        Token::StringLiteral(s)
    }

    /// Reads an identifier or keyword. Identifiers are uppercased for case-insensitive
    /// matching. If followed by a BASIC type sigil ($, %, !, #), the sigil is included.
    /// Known keywords (LET, PRINT, IF, etc.) are returned as their specific token variants;
    /// REM consumes the rest of the line as a comment.
    fn read_identifier_or_keyword(&mut self) -> Token {
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        // Check for type sigil
        if let Some(ch) = self.peek() {
            if ch == '$' || ch == '%' || ch == '!' || ch == '#' {
                s.push(ch);
                self.advance();
            }
        }
        let upper = s.to_uppercase();
        match upper.as_str() {
            "LET" => Token::Let,
            "PRINT" => Token::Print,
            "IF" => Token::If,
            "THEN" => Token::Then,
            "ELSE" => Token::Else,
            "GOTO" => Token::Goto,
            "INPUT" => Token::Input,
            "FOR" => Token::For,
            "TO" => Token::To,
            "STEP" => Token::Step,
            "NEXT" => Token::Next,
            "END" => Token::End,
            "DEF" => Token::Def,
            "AND" => Token::And,
            "OR" => Token::Or,
            "XOR" => Token::Xor,
            "NOT" => Token::Not,
            "REM" => {
                // Consume rest of line as remark
                let mut comment = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    comment.push(ch);
                    self.advance();
                }
                Token::Rem(comment.trim().to_string())
            }
            _ => Token::Identifier(upper),
        }
    }

    /// Tokenizes the entire input, returning a vector of tokens ending with `Token::Eof`.
    /// Unknown characters are skipped with a warning printed to stderr.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => {
                    tokens.push(Token::Eof);
                    break;
                }
                Some('\n') => {
                    self.advance();
                    tokens.push(Token::Newline);
                }
                Some('\'') => {
                    self.advance();
                    let mut comment = String::new();
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        comment.push(ch);
                        self.advance();
                    }
                    tokens.push(Token::Rem(comment.trim().to_string()));
                }
                Some('"') => tokens.push(self.read_string()),
                Some('+') => {
                    self.advance();
                    tokens.push(Token::Plus);
                }
                Some('-') => {
                    self.advance();
                    tokens.push(Token::Minus);
                }
                Some('*') => {
                    self.advance();
                    tokens.push(Token::Star);
                }
                Some('/') => {
                    self.advance();
                    tokens.push(Token::Slash);
                }
                Some('^') => {
                    self.advance();
                    tokens.push(Token::Caret);
                }
                Some('=') => {
                    self.advance();
                    tokens.push(Token::Equal);
                }
                Some('<') => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token::NotEqual);
                    } else if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::LessEqual);
                    } else {
                        tokens.push(Token::Less);
                    }
                }
                Some('>') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::GreaterEqual);
                    } else {
                        tokens.push(Token::Greater);
                    }
                }
                Some('(') => {
                    self.advance();
                    tokens.push(Token::LeftParen);
                }
                Some(')') => {
                    self.advance();
                    tokens.push(Token::RightParen);
                }
                Some(',') => {
                    self.advance();
                    tokens.push(Token::Comma);
                }
                Some(';') => {
                    self.advance();
                    tokens.push(Token::Semicolon);
                }
                Some(':') => {
                    self.advance();
                    tokens.push(Token::Colon);
                }
                Some(ch) if ch.is_ascii_digit() => {
                    tokens.push(self.read_number());
                }
                Some(ch) if ch.is_ascii_alphabetic() => {
                    tokens.push(self.read_identifier_or_keyword());
                }
                Some(ch) => {
                    // Skip unknown characters
                    self.advance();
                    eprintln!("Warning: skipping unknown character '{}'", ch);
                }
            }
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_number() {
        let tokens = Lexer::new("42").tokenize();
        assert_eq!(tokens, vec![Token::Number(42.0), Token::Eof]);
    }

    #[test]
    fn test_tokenize_float() {
        let tokens = Lexer::new("3.14").tokenize();
        assert_eq!(tokens, vec![Token::Number(3.14), Token::Eof]);
    }

    #[test]
    fn test_tokenize_string() {
        let tokens = Lexer::new("\"HELLO\"").tokenize();
        assert_eq!(tokens, vec![Token::StringLiteral("HELLO".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_operators() {
        let tokens = Lexer::new("+ - * / ^ = <> < > <= >=").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Caret,
                Token::Equal,
                Token::NotEqual,
                Token::Less,
                Token::Greater,
                Token::LessEqual,
                Token::GreaterEqual,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_keywords() {
        let tokens = Lexer::new("LET PRINT IF THEN ELSE GOTO INPUT FOR TO STEP NEXT END").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Let,
                Token::Print,
                Token::If,
                Token::Then,
                Token::Else,
                Token::Goto,
                Token::Input,
                Token::For,
                Token::To,
                Token::Step,
                Token::Next,
                Token::End,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_logical_keywords() {
        let tokens = Lexer::new("AND OR XOR NOT").tokenize();
        assert_eq!(tokens, vec![Token::And, Token::Or, Token::Xor, Token::Not, Token::Eof]);
    }

    #[test]
    fn test_tokenize_logical_keywords_case_insensitive() {
        let tokens = Lexer::new("and or xor not").tokenize();
        assert_eq!(tokens, vec![Token::And, Token::Or, Token::Xor, Token::Not, Token::Eof]);
    }

    #[test]
    fn test_tokenize_logical_in_expression() {
        let tokens = Lexer::new("X > 0 AND Y < 10").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("X".to_string()),
                Token::Greater,
                Token::Number(0.0),
                Token::And,
                Token::Identifier("Y".to_string()),
                Token::Less,
                Token::Number(10.0),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_case_insensitive() {
        let tokens = Lexer::new("let print if").tokenize();
        assert_eq!(tokens, vec![Token::Let, Token::Print, Token::If, Token::Eof]);
    }

    #[test]
    fn test_tokenize_variable_with_sigil() {
        let tokens = Lexer::new("N$ X% A! B#").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("N$".to_string()),
                Token::Identifier("X%".to_string()),
                Token::Identifier("A!".to_string()),
                Token::Identifier("B#".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_remark() {
        let tokens = Lexer::new("REM THIS IS A COMMENT").tokenize();
        assert_eq!(tokens, vec![Token::Rem("THIS IS A COMMENT".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_basic_line() {
        let tokens = Lexer::new("10 PRINT \"HELLO\"\n").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Number(10.0),
                Token::Print,
                Token::StringLiteral("HELLO".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_expression() {
        let tokens = Lexer::new("(A + B) * 2").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::LeftParen,
                Token::Identifier("A".to_string()),
                Token::Plus,
                Token::Identifier("B".to_string()),
                Token::RightParen,
                Token::Star,
                Token::Number(2.0),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_delimiters() {
        let tokens = Lexer::new(", ; :").tokenize();
        assert_eq!(tokens, vec![Token::Comma, Token::Semicolon, Token::Colon, Token::Eof]);
    }

    #[test]
    fn test_tokenize_leading_whitespace() {
        let tokens = Lexer::new("   10 PRINT \"HI\"\n").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Number(10.0),
                Token::Print,
                Token::StringLiteral("HI".to_string()),
                Token::Newline,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_empty_lines() {
        let tokens = Lexer::new("\n\n10 END\n\n").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Newline,
                Token::Newline,
                Token::Number(10.0),
                Token::End,
                Token::Newline,
                Token::Newline,
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_tokenize_apostrophe_comment() {
        let tokens = Lexer::new("' THIS IS A COMMENT").tokenize();
        assert_eq!(tokens, vec![Token::Rem("THIS IS A COMMENT".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_apostrophe_comment_with_newline() {
        let tokens = Lexer::new("' COMMENT\n10 END").tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Rem("COMMENT".to_string()),
                Token::Newline,
                Token::Number(10.0),
                Token::End,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_unknown_character() {
        let tokens = Lexer::new("@").tokenize();
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_tokenize_identifier_with_underscore() {
        let tokens = Lexer::new("MY_VAR").tokenize();
        assert_eq!(tokens, vec![Token::Identifier("MY_VAR".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_number_with_trailing_dot() {
        let tokens = Lexer::new("42.").tokenize();
        assert_eq!(tokens, vec![Token::Number(42.0), Token::Eof]);
    }

    #[test]
    fn test_tokenize_unterminated_string() {
        let tokens = Lexer::new("\"HELLO").tokenize();
        assert_eq!(tokens, vec![Token::StringLiteral("HELLO".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_empty_string() {
        let tokens = Lexer::new("\"\"").tokenize();
        assert_eq!(tokens, vec![Token::StringLiteral("".to_string()), Token::Eof]);
    }

    #[test]
    fn test_tokenize_tabs_and_carriage_returns() {
        let tokens = Lexer::new("\t\r10").tokenize();
        assert_eq!(tokens, vec![Token::Number(10.0), Token::Eof]);
    }
}
