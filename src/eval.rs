use crate::expr::{BinOp, Expr};
use std::collections::HashMap;

/// Runtime values
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
}

impl Value {
    pub fn as_number(&self) -> Result<f64, String> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::String(s) => Err(format!("Expected number, got string: \"{}\"", s)),
        }
    }

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

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
        }
    }
}

impl std::fmt::Display for Value {
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

/// Stack-based expression evaluator
pub struct Evaluator {
    pub variables: HashMap<String, Value>,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            variables: HashMap::new(),
        }
    }

    pub fn eval_expr(&self, expr: &Expr) -> Result<Value, String> {
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

    fn eval_binary_op(&self, op: &BinOp, left: &Value, right: &Value) -> Result<Value, String> {
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

    fn eval_function(&self, name: &str, args: &[Expr]) -> Result<Value, String> {
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
                // Simple pseudo-random: always return a fixed value for deterministic testing
                // In a real interpreter, use a PRNG
                if args.len() != 1 {
                    return Err("RND expects 1 argument".to_string());
                }
                // For now, return a pseudo-random value
                Ok(Value::Number(0.5))
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
        let evaluator = Evaluator::new();
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
    fn test_eval_abs_function() {
        assert_eq!(eval("ABS(-5)"), Value::Number(5.0));
        assert_eq!(eval("ABS(5)"), Value::Number(5.0));
    }

    #[test]
    fn test_eval_sqr_function() {
        assert_eq!(eval("SQR(9)"), Value::Number(3.0));
        assert_eq!(eval("SQR(16)"), Value::Number(4.0));
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
        let evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_undefined_variable() {
        let tokens = Lexer::new("UNDEFINED").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let evaluator = Evaluator::new();
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
