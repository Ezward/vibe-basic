//! Expression evaluation and runtime value representation for BASIC.
//!
//! This module provides the `Value` enum for runtime values (numbers and strings),
//! and the `Evaluator` struct which evaluates parsed expression trees against a
//! variable store. It supports arithmetic, string concatenation, comparison operators
//! (returning MS-BASIC style -1/0), and built-in functions (INT, ABS, SQR, RND, LEN).

use crate::expr::{BinOp, Expr};
use rand::Rng;
use std::collections::HashMap;

/// A runtime value: either a floating-point number or a string.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
}

impl Value {
    /// Extracts the numeric value, or returns an error if this is a string.
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::String(s) => Err(format!("Expected number, got string: \"{}\"", s)),
        }
    }

    /// Formats the value for PRINT output using MS-BASIC conventions:
    /// positive numbers get a leading space, all numbers get a trailing space,
    /// and integer-valued floats are printed without a decimal point.
    pub fn to_print_string(&self) -> String {
        match self {
            Value::Number(n) => {
                if *n >= 0.0 {
                    // MS-BASIC prints a leading space for positive numbers
                    if *n == (*n as i64 as f64) {
                        format!(" {} ", *n as i64)
                    } else {
                        format!(" {} ", n)
                    }
                } else if *n == (*n as i64 as f64) {
                    format!("{} ", *n as i64)
                } else {
                    format!("{} ", n)
                }
            }
            Value::String(s) => s.clone(),
        }
    }

    /// Returns whether the value is truthy: non-zero for numbers, non-empty for strings.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
        }
    }
}

impl std::fmt::Display for Value {
    /// Displays the value: integers without a decimal point, strings as-is.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => {
                if *n == (*n as i64 as f64) {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(s) => write!(f, "{}", s),
        }
    }
}

/// Expression evaluator with a variable store and random number generator.
pub struct Evaluator {
    pub variables: HashMap<String, Value>,
    rng: rand::rngs::ThreadRng,
}

impl Evaluator {
    /// Creates a new evaluator with an empty variable store.
    pub fn new() -> Self {
        Evaluator {
            variables: HashMap::new(),
            rng: rand::thread_rng(),
        }
    }

    /// Recursively evaluates an expression tree, returning a runtime `Value`.
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Number(n) => Ok(Value::Number(*n)),
            Expr::StringLiteral(s) => Ok(Value::String(s.clone())),
            Expr::Variable(name) => self
                .variables
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Undefined variable: {}", name)),
            Expr::UnaryMinus(inner) => {
                let val = self.eval_expr(inner)?;
                Ok(Value::Number(-val.as_number()?))
            }
            Expr::BinaryOp { op, left, right } => {
                let lval = self.eval_expr(left)?;
                let rval = self.eval_expr(right)?;
                self.eval_binary_op(op, &lval, &rval)
            }
            Expr::FunctionCall { name, args } => self.eval_function(name, args),
        }
    }

    /// Evaluates a binary operation. Handles string concatenation with `+`, string
    /// comparisons, numeric arithmetic, and numeric comparisons (returning -1 for
    /// true and 0 for false, per MS-BASIC convention).
    fn eval_binary_op(&mut self, op: &BinOp, left: &Value, right: &Value) -> Result<Value, String> {
        // String concatenation with +
        if *op == BinOp::Add {
            if let (Value::String(l), Value::String(r)) = (left, right) {
                return Ok(Value::String(format!("{}{}", l, r)));
            }
        }
        // String comparison
        if let (Value::String(l), Value::String(r)) = (left, right) {
            let result = match op {
                BinOp::Equal => l == r,
                BinOp::NotEqual => l != r,
                BinOp::Less => l < r,
                BinOp::Greater => l > r,
                BinOp::LessEqual => l <= r,
                BinOp::GreaterEqual => l >= r,
                _ => return Err(format!("Cannot apply {:?} to strings", op)),
            };
            return Ok(Value::Number(if result { -1.0 } else { 0.0 }));
        }

        let l = left.as_number()?;
        let r = right.as_number()?;
        let result = match op {
            BinOp::Add => Value::Number(l + r),
            BinOp::Sub => Value::Number(l - r),
            BinOp::Mul => Value::Number(l * r),
            BinOp::Div => {
                if r == 0.0 {
                    return Err("Division by zero".to_string());
                }
                Value::Number(l / r)
            }
            BinOp::Pow => Value::Number(l.powf(r)),
            BinOp::Equal => Value::Number(if l == r { -1.0 } else { 0.0 }),
            BinOp::NotEqual => Value::Number(if l != r { -1.0 } else { 0.0 }),
            BinOp::Less => Value::Number(if l < r { -1.0 } else { 0.0 }),
            BinOp::Greater => Value::Number(if l > r { -1.0 } else { 0.0 }),
            BinOp::LessEqual => Value::Number(if l <= r { -1.0 } else { 0.0 }),
            BinOp::GreaterEqual => Value::Number(if l >= r { -1.0 } else { 0.0 }),
        };
        Ok(result)
    }

    /// Evaluates a built-in function call. Supported functions:
    /// INT (floor), ABS (absolute value), SQR (square root), RND (random [0,1)),
    /// LEN (string length).
    fn eval_function(&mut self, name: &str, args: &[Expr]) -> Result<Value, String> {
        match name {
            "INT" => {
                if args.len() != 1 {
                    return Err("INT expects 1 argument".to_string());
                }
                let val = self.eval_expr(&args[0])?.as_number()?;
                Ok(Value::Number(val.floor()))
            }
            "ABS" => {
                if args.len() != 1 {
                    return Err("ABS expects 1 argument".to_string());
                }
                let val = self.eval_expr(&args[0])?.as_number()?;
                Ok(Value::Number(val.abs()))
            }
            "SQR" => {
                if args.len() != 1 {
                    return Err("SQR expects 1 argument".to_string());
                }
                let val = self.eval_expr(&args[0])?.as_number()?;
                Ok(Value::Number(val.sqrt()))
            }
            "RND" => {
                if args.len() != 1 {
                    return Err("RND expects 1 argument".to_string());
                }
                let _arg = self.eval_expr(&args[0])?.as_number()?;
                // MS-BASIC RND returns a random float in [0.0, 1.0)
                let val: f64 = self.rng.gen();
                Ok(Value::Number(val))
            }
            "LEN" => {
                if args.len() != 1 {
                    return Err("LEN expects 1 argument".to_string());
                }
                let val = self.eval_expr(&args[0])?;
                match val {
                    Value::String(s) => Ok(Value::Number(s.len() as f64)),
                    _ => Err("LEN expects a string argument".to_string()),
                }
            }
            _ => Err(format!("Unknown function: {}", name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::ExprParser;
    use crate::token::Lexer;

    fn eval(input: &str) -> Value {
        let tokens = Lexer::new(input).tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        evaluator.eval_expr(&expr).unwrap()
    }

    fn eval_with_vars(input: &str, vars: Vec<(&str, Value)>) -> Value {
        let tokens = Lexer::new(input).tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        for (name, val) in vars {
            evaluator.variables.insert(name.to_string(), val);
        }
        evaluator.eval_expr(&expr).unwrap()
    }

    #[test]
    fn test_eval_number() {
        assert_eq!(eval("42"), Value::Number(42.0));
    }

    #[test]
    fn test_eval_float() {
        assert_eq!(eval("3.14"), Value::Number(3.14));
    }

    #[test]
    fn test_eval_string() {
        assert_eq!(eval("\"HELLO\""), Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_eval_addition() {
        assert_eq!(eval("2 + 3"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_subtraction() {
        assert_eq!(eval("10 - 4"), Value::Number(6.0));
    }

    #[test]
    fn test_eval_multiplication() {
        assert_eq!(eval("3 * 7"), Value::Number(21.0));
    }

    #[test]
    fn test_eval_division() {
        assert_eq!(eval("20 / 4"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_power() {
        assert_eq!(eval("2 ^ 3"), Value::Number(8.0));
    }

    #[test]
    fn test_eval_precedence() {
        // 2 + 3 * 4 = 14
        assert_eq!(eval("2 + 3 * 4"), Value::Number(14.0));
    }

    #[test]
    fn test_eval_parentheses() {
        // (2 + 3) * 4 = 20
        assert_eq!(eval("(2 + 3) * 4"), Value::Number(20.0));
    }

    #[test]
    fn test_eval_unary_minus() {
        assert_eq!(eval("-5"), Value::Number(-5.0));
    }

    #[test]
    fn test_eval_unary_minus_in_expr() {
        assert_eq!(eval("-3 + 7"), Value::Number(4.0));
    }

    #[test]
    fn test_eval_nested_parens() {
        // ((2 + 3) * (4 - 1)) = 15
        assert_eq!(eval("((2 + 3) * (4 - 1))"), Value::Number(15.0));
    }

    #[test]
    fn test_eval_variable() {
        let result = eval_with_vars("X", vec![("X", Value::Number(10.0))]);
        assert_eq!(result, Value::Number(10.0));
    }

    #[test]
    fn test_eval_variable_expression() {
        let result = eval_with_vars("X + Y", vec![("X", Value::Number(5.0)), ("Y", Value::Number(3.0))]);
        assert_eq!(result, Value::Number(8.0));
    }

    #[test]
    fn test_eval_string_variable() {
        let result = eval_with_vars("N$", vec![("N$", Value::String("ALICE".to_string()))]);
        assert_eq!(result, Value::String("ALICE".to_string()));
    }

    #[test]
    fn test_eval_comparison_equal_true() {
        assert_eq!(eval("5 = 5"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_comparison_equal_false() {
        assert_eq!(eval("5 = 3"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_comparison_not_equal() {
        assert_eq!(eval("5 <> 3"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_comparison_less() {
        assert_eq!(eval("3 < 5"), Value::Number(-1.0));
        assert_eq!(eval("5 < 3"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_comparison_greater() {
        assert_eq!(eval("5 > 3"), Value::Number(-1.0));
        assert_eq!(eval("3 > 5"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_comparison_less_equal() {
        assert_eq!(eval("3 <= 5"), Value::Number(-1.0));
        assert_eq!(eval("5 <= 5"), Value::Number(-1.0));
        assert_eq!(eval("6 <= 5"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_comparison_greater_equal() {
        assert_eq!(eval("5 >= 3"), Value::Number(-1.0));
        assert_eq!(eval("5 >= 5"), Value::Number(-1.0));
        assert_eq!(eval("4 >= 5"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_int_function() {
        assert_eq!(eval("INT(3.7)"), Value::Number(3.0));
        assert_eq!(eval("INT(-2.3)"), Value::Number(-3.0));
    }

    #[test]
    fn test_eval_int_zero() {
        assert_eq!(eval("INT(0.0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_int_already_integer() {
        assert_eq!(eval("INT(5)"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_int_negative_exact() {
        assert_eq!(eval("INT(-4)"), Value::Number(-4.0));
    }

    #[test]
    fn test_eval_int_with_expression() {
        assert_eq!(eval("INT(7 / 2)"), Value::Number(3.0));
    }

    #[test]
    fn test_eval_int_wrong_arg_count() {
        let tokens = Lexer::new("INT()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_abs_function() {
        assert_eq!(eval("ABS(-5)"), Value::Number(5.0));
        assert_eq!(eval("ABS(5)"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_abs_zero() {
        assert_eq!(eval("ABS(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_abs_float() {
        assert_eq!(eval("ABS(-3.14)"), Value::Number(3.14));
    }

    #[test]
    fn test_eval_abs_with_expression() {
        assert_eq!(eval("ABS(3 - 10)"), Value::Number(7.0));
    }

    #[test]
    fn test_eval_abs_wrong_arg_count() {
        let tokens = Lexer::new("ABS()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_sqr_function() {
        assert_eq!(eval("SQR(9)"), Value::Number(3.0));
        assert_eq!(eval("SQR(16)"), Value::Number(4.0));
    }

    #[test]
    fn test_eval_sqr_zero() {
        assert_eq!(eval("SQR(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_sqr_one() {
        assert_eq!(eval("SQR(1)"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_sqr_non_perfect() {
        let result = eval("SQR(2)");
        if let Value::Number(n) = result {
            assert!((n - std::f64::consts::SQRT_2).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_sqr_with_expression() {
        assert_eq!(eval("SQR(4 + 5)"), Value::Number(3.0));
    }

    #[test]
    fn test_eval_sqr_wrong_arg_count() {
        let tokens = Lexer::new("SQR()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_rnd_returns_in_range() {
        // RND(1) should return a value in [0.0, 1.0)
        for _ in 0..100 {
            let result = eval("RND(1)");
            if let Value::Number(n) = result {
                assert!(n >= 0.0, "RND returned {} which is < 0.0", n);
                assert!(n < 1.0, "RND returned {} which is >= 1.0", n);
            } else {
                panic!("Expected number from RND");
            }
        }
    }

    #[test]
    fn test_eval_rnd_produces_varying_values() {
        // Run RND many times and verify we don't always get the same value
        let mut values = std::collections::HashSet::new();
        for _ in 0..50 {
            if let Value::Number(n) = eval("RND(1)") {
                values.insert(n.to_bits());
            }
        }
        assert!(values.len() > 1, "RND returned the same value every time");
    }

    #[test]
    fn test_eval_rnd_wrong_arg_count() {
        let tokens = Lexer::new("RND()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_rnd_in_expression() {
        // INT(RND(1) * 10) should produce an integer in [0, 9]
        for _ in 0..100 {
            let result = eval("INT(RND(1) * 10)");
            if let Value::Number(n) = result {
                assert!(n >= 0.0 && n <= 9.0, "INT(RND(1)*10) returned {}", n);
                assert_eq!(n, n.floor(), "Expected integer, got {}", n);
            } else {
                panic!("Expected number");
            }
        }
    }

    #[test]
    fn test_eval_len_function() {
        assert_eq!(eval("LEN(\"HELLO\")"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_len_empty_string() {
        assert_eq!(eval("LEN(\"\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_len_single_char() {
        assert_eq!(eval("LEN(\"A\")"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_len_with_spaces() {
        assert_eq!(eval("LEN(\"A B C\")"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_len_wrong_type() {
        let tokens = Lexer::new("LEN(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_len_wrong_arg_count() {
        let tokens = Lexer::new("LEN()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_unknown_function() {
        let tokens = Lexer::new("FOO(1)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_string_concatenation() {
        assert_eq!(eval("\"HELLO\" + \" WORLD\""), Value::String("HELLO WORLD".to_string()));
    }

    #[test]
    fn test_eval_string_comparison() {
        assert_eq!(eval("\"ABC\" = \"ABC\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" = \"DEF\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_division_by_zero() {
        let tokens = Lexer::new("1 / 0").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_undefined_variable() {
        let tokens = Lexer::new("UNDEFINED").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_complex_expression() {
        // (10 + 20) * 2 = 60
        assert_eq!(eval("(10 + 20) * 2"), Value::Number(60.0));
    }

    #[test]
    fn test_eval_multiple_operations() {
        // 2 ^ 3 + 1 = 9
        assert_eq!(eval("2 ^ 3 + 1"), Value::Number(9.0));
    }

    #[test]
    fn test_value_print_string_positive_int() {
        assert_eq!(Value::Number(5.0).to_print_string(), " 5 ");
    }

    #[test]
    fn test_value_print_string_negative_int() {
        assert_eq!(Value::Number(-3.0).to_print_string(), "-3 ");
    }

    #[test]
    fn test_value_print_string_string() {
        assert_eq!(Value::String("HELLO".to_string()).to_print_string(), "HELLO");
    }

    #[test]
    fn test_value_is_truthy() {
        assert!(Value::Number(1.0).is_truthy());
        assert!(Value::Number(-1.0).is_truthy());
        assert!(!Value::Number(0.0).is_truthy());
        assert!(Value::String("HI".to_string()).is_truthy());
        assert!(!Value::String("".to_string()).is_truthy());
    }
}
