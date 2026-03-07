//! Expression evaluation and runtime value representation for BASIC.
//!
//! This module provides the `Value` enum for runtime values (numbers and strings),
//! and the `Evaluator` struct which evaluates parsed expression trees against a
//! variable store. It supports arithmetic, string concatenation, comparison operators
//! (returning MS-BASIC style -1/0), logical operators (AND, OR, XOR, NOT as bitwise
//! integer operations), and built-in functions including numeric (INT, ABS, SQR, RND,
//! EXP, LOG, SGN, SIN, COS, TAN, ATN, FIX, CINT, CSNG, CDBL), string (LEN, LEFT$,
//! RIGHT$, MID$, INSTR, ASC, CHR$, STR$, VAL, HEX$, OCT$, STRING$, SPACE$, SPC, TAB),
//! binary data (MKI$, MKS$, MKD$, CVI, CVS, CVD), and user-defined functions (DEF FN).

use crate::expr::{BinOp, Expr};
use rand::Rng;
use std::collections::HashMap;

/// A user-defined function created with DEF FN.
#[derive(Debug, Clone)]
pub struct UserFunction {
    pub params: Vec<String>,
    pub body: Expr,
}

/// A work item on the evaluation stack. Each step is either an expression to
/// evaluate or a pending operation waiting for its operands on the value stack.
enum EvalStep<'a> {
    /// Evaluate this expression node, pushing its result onto the value stack.
    Eval(&'a Expr),
    /// Pop one value, negate it, push the result.
    ApplyUnaryMinus,
    /// Pop one value, apply bitwise NOT, push the result.
    ApplyUnaryNot,
    /// Pop two values (right then left), apply the operator, push the result.
    ApplyBinaryOp(&'a BinOp),
    /// Pop `arg_count` values, call the named built-in function, push the result.
    ApplyFunction(&'a str, usize),
}

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
    pub user_functions: HashMap<String, UserFunction>,
    rng: rand::rngs::ThreadRng,
}

impl Evaluator {
    /// Creates a new evaluator with an empty variable store.
    pub fn new() -> Self {
        Evaluator {
            variables: HashMap::new(),
            user_functions: HashMap::new(),
            rng: rand::thread_rng(),
        }
    }

    /// Evaluates an expression tree using an explicit work stack and value stack,
    /// returning a runtime `Value`. This iterative approach enables future debugger
    /// support for single-stepping through expression evaluation.
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        let mut work: Vec<EvalStep> = vec![EvalStep::Eval(expr)];
        let mut values: Vec<Value> = Vec::new();

        while let Some(step) = work.pop() {
            match step {
                EvalStep::Eval(e) => match e {
                    Expr::Number(n) => values.push(Value::Number(*n)),
                    Expr::StringLiteral(s) => values.push(Value::String(s.clone())),
                    Expr::Variable(name) => {
                        if let Some(val) = self.variables.get(name).cloned() {
                            values.push(val);
                        } else if let Some(func) = self.user_functions.get(name).cloned() {
                            // Parameterless user-defined function called without parens
                            if !func.params.is_empty() {
                                return Err(format!("{} expects {} argument(s)", name, func.params.len()));
                            }
                            let result = self.eval_expr(&func.body)?;
                            values.push(result);
                        } else {
                            return Err(format!("Undefined variable: {}", name));
                        }
                    }
                    Expr::UnaryMinus(inner) => {
                        work.push(EvalStep::ApplyUnaryMinus);
                        work.push(EvalStep::Eval(inner));
                    }
                    Expr::UnaryNot(inner) => {
                        work.push(EvalStep::ApplyUnaryNot);
                        work.push(EvalStep::Eval(inner));
                    }
                    Expr::BinaryOp { op, left, right } => {
                        work.push(EvalStep::ApplyBinaryOp(op));
                        work.push(EvalStep::Eval(right));
                        work.push(EvalStep::Eval(left));
                    }
                    Expr::FunctionCall { name, args } => {
                        work.push(EvalStep::ApplyFunction(name, args.len()));
                        for arg in args.iter().rev() {
                            work.push(EvalStep::Eval(arg));
                        }
                    }
                },
                EvalStep::ApplyUnaryMinus => {
                    let val = values.pop().expect("value stack underflow").as_number()?;
                    values.push(Value::Number(-val));
                }
                EvalStep::ApplyUnaryNot => {
                    let val = values.pop().expect("value stack underflow").as_number()?;
                    let int_val = val as i64;
                    values.push(Value::Number(!int_val as f64));
                }
                EvalStep::ApplyBinaryOp(op) => {
                    let rval = values.pop().expect("value stack underflow");
                    let lval = values.pop().expect("value stack underflow");
                    values.push(self.eval_binary_op(op, &lval, &rval)?);
                }
                EvalStep::ApplyFunction(name, arg_count) => {
                    let start = values.len() - arg_count;
                    let args: Vec<Value> = values.drain(start..).collect();
                    values.push(self.apply_function(name, &args)?);
                }
            }
        }

        Ok(values.pop().expect("value stack empty after evaluation"))
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
            BinOp::And => Value::Number((l as i64 & r as i64) as f64),
            BinOp::Or => Value::Number((l as i64 | r as i64) as f64),
            BinOp::Xor => Value::Number((l as i64 ^ r as i64) as f64),
        };
        Ok(result)
    }

    /// Applies a built-in or user-defined function to pre-evaluated argument values.
    /// Supported built-in functions include numeric (INT, ABS, SQR, RND, EXP, LOG,
    /// SGN, SIN, COS, TAN, ATN, FIX, CINT, CSNG, CDBL), string (LEN, LEFT$, RIGHT$,
    /// MID$, INSTR, ASC, CHR$, STR$, VAL, HEX$, OCT$, STRING$, SPACE$, SPC, TAB),
    /// and binary data (MKI$, MKS$, MKD$, CVI, CVS, CVD). Falls back to user-defined
    /// functions registered via DEF FN.
    fn apply_function(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "INT" => {
                if args.len() != 1 {
                    return Err("INT expects 1 argument".to_string());
                }
                let val = args[0].as_number()?;
                Ok(Value::Number(val.floor()))
            }
            "ABS" => {
                if args.len() != 1 {
                    return Err("ABS expects 1 argument".to_string());
                }
                let val = args[0].as_number()?;
                Ok(Value::Number(val.abs()))
            }
            "SQR" => {
                if args.len() != 1 {
                    return Err("SQR expects 1 argument".to_string());
                }
                let val = args[0].as_number()?;
                Ok(Value::Number(val.sqrt()))
            }
            "RND" => {
                if args.len() != 1 {
                    return Err("RND expects 1 argument".to_string());
                }
                let _arg = args[0].as_number()?;
                // MS-BASIC RND returns a random float in [0.0, 1.0)
                let val: f64 = self.rng.gen();
                Ok(Value::Number(val))
            }
            "EXP" => {
                if args.len() != 1 {
                    return Err("EXP expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.exp()))
            }
            "LOG" => {
                if args.len() != 1 {
                    return Err("LOG expects 1 argument".to_string());
                }
                let x = args[0].as_number()?;
                if x <= 0.0 {
                    return Err("LOG requires a positive argument".to_string());
                }
                Ok(Value::Number(x.ln()))
            }
            "SGN" => {
                if args.len() != 1 {
                    return Err("SGN expects 1 argument".to_string());
                }
                let x = args[0].as_number()?;
                let result = if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                };
                Ok(Value::Number(result))
            }
            "SIN" => {
                if args.len() != 1 {
                    return Err("SIN expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.sin()))
            }
            "COS" => {
                if args.len() != 1 {
                    return Err("COS expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.cos()))
            }
            "TAN" => {
                if args.len() != 1 {
                    return Err("TAN expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.tan()))
            }
            "ATN" => {
                if args.len() != 1 {
                    return Err("ATN expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.atan()))
            }
            "FIX" => {
                if args.len() != 1 {
                    return Err("FIX expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.trunc()))
            }
            "CINT" => {
                if args.len() != 1 {
                    return Err("CINT expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?.round()))
            }
            "CSNG" => {
                if args.len() != 1 {
                    return Err("CSNG expects 1 argument".to_string());
                }
                let x = args[0].as_number()?;
                Ok(Value::Number((x as f32) as f64))
            }
            "CDBL" => {
                if args.len() != 1 {
                    return Err("CDBL expects 1 argument".to_string());
                }
                Ok(Value::Number(args[0].as_number()?))
            }
            "LEN" => {
                if args.len() != 1 {
                    return Err("LEN expects 1 argument".to_string());
                }
                match &args[0] {
                    Value::String(s) => Ok(Value::Number(s.len() as f64)),
                    _ => Err("LEN expects a string argument".to_string()),
                }
            }

            // --- Substring and Search Functions ---
            "LEFT$" => {
                if args.len() != 2 {
                    return Err("LEFT$ expects 2 arguments".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("LEFT$ expects a string as first argument".to_string()),
                };
                let n = args[1].as_number()? as usize;
                let result: String = s.chars().take(n).collect();
                Ok(Value::String(result))
            }
            "RIGHT$" => {
                if args.len() != 2 {
                    return Err("RIGHT$ expects 2 arguments".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("RIGHT$ expects a string as first argument".to_string()),
                };
                let n = args[1].as_number()? as usize;
                let chars: Vec<char> = s.chars().collect();
                let start = chars.len().saturating_sub(n);
                let result: String = chars[start..].iter().collect();
                Ok(Value::String(result))
            }
            "MID$" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err("MID$ expects 2 or 3 arguments".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("MID$ expects a string as first argument".to_string()),
                };
                let n = args[1].as_number()? as usize;
                if n == 0 {
                    return Err("MID$ position must be >= 1".to_string());
                }
                let chars: Vec<char> = s.chars().collect();
                let start = (n - 1).min(chars.len()); // 1-based to 0-based
                if args.len() == 3 {
                    let m = args[2].as_number()? as usize;
                    let end = (start + m).min(chars.len());
                    Ok(Value::String(chars[start..end].iter().collect()))
                } else {
                    Ok(Value::String(chars[start..].iter().collect()))
                }
            }
            "INSTR" => {
                // INSTR([n,] x$, y$) — 2 or 3 arguments
                if args.len() == 2 {
                    let x = match &args[0] {
                        Value::String(s) => s.clone(),
                        _ => return Err("INSTR expects a string argument".to_string()),
                    };
                    let y = match &args[1] {
                        Value::String(s) => s.clone(),
                        _ => return Err("INSTR expects a string argument".to_string()),
                    };
                    match x.find(&y) {
                        Some(pos) => Ok(Value::Number((pos + 1) as f64)), // 1-based
                        None => Ok(Value::Number(0.0)),
                    }
                } else if args.len() == 3 {
                    let n = args[0].as_number()? as usize;
                    if n == 0 {
                        return Err("INSTR start position must be >= 1".to_string());
                    }
                    let x = match &args[1] {
                        Value::String(s) => s.clone(),
                        _ => return Err("INSTR expects a string argument".to_string()),
                    };
                    let y = match &args[2] {
                        Value::String(s) => s.clone(),
                        _ => return Err("INSTR expects a string argument".to_string()),
                    };
                    let start = (n - 1).min(x.len());
                    match x[start..].find(&y) {
                        Some(pos) => Ok(Value::Number((pos + start + 1) as f64)), // 1-based
                        None => Ok(Value::Number(0.0)),
                    }
                } else {
                    Err("INSTR expects 2 or 3 arguments".to_string())
                }
            }

            // --- Conversion Functions ---
            "ASC" => {
                if args.len() != 1 {
                    return Err("ASC expects 1 argument".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("ASC expects a string argument".to_string()),
                };
                if s.is_empty() {
                    return Err("Illegal function call: ASC of empty string".to_string());
                }
                Ok(Value::Number(s.as_bytes()[0] as f64))
            }
            "CHR$" => {
                if args.len() != 1 {
                    return Err("CHR$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as u8;
                Ok(Value::String(String::from(n as char)))
            }
            "STR$" => {
                if args.len() != 1 {
                    return Err("STR$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()?;
                // GW-BASIC STR$ includes a leading space for positive numbers
                let s = if n >= 0.0 {
                    if n == (n as i64 as f64) {
                        format!(" {}", n as i64)
                    } else {
                        format!(" {}", n)
                    }
                } else if n == (n as i64 as f64) {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                };
                Ok(Value::String(s))
            }
            "VAL" => {
                if args.len() != 1 {
                    return Err("VAL expects 1 argument".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.trim().to_string(),
                    _ => return Err("VAL expects a string argument".to_string()),
                };
                let n = s.parse::<f64>().unwrap_or(0.0);
                Ok(Value::Number(n))
            }
            "HEX$" => {
                if args.len() != 1 {
                    return Err("HEX$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as i64;
                Ok(Value::String(format!("{:X}", n)))
            }
            "OCT$" => {
                if args.len() != 1 {
                    return Err("OCT$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as i64;
                Ok(Value::String(format!("{:o}", n)))
            }

            // --- Formatting and Creation Functions ---
            "STRING$" => {
                if args.len() != 2 {
                    return Err("STRING$ expects 2 arguments".to_string());
                }
                let n = args[0].as_number()? as usize;
                let ch = match &args[1] {
                    Value::String(s) => {
                        if s.is_empty() {
                            return Err("Illegal function call: STRING$ with empty string".to_string());
                        }
                        s.chars().next().unwrap()
                    }
                    Value::Number(m) => *m as u8 as char,
                };
                Ok(Value::String(std::iter::repeat_n(ch, n).collect()))
            }
            "SPACE$" => {
                if args.len() != 1 {
                    return Err("SPACE$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as usize;
                Ok(Value::String(" ".repeat(n)))
            }
            "SPC" => {
                if args.len() != 1 {
                    return Err("SPC expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as usize;
                Ok(Value::String(" ".repeat(n)))
            }
            "TAB" => {
                if args.len() != 1 {
                    return Err("TAB expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as usize;
                // TAB returns spaces to reach column n (simplified: just returns n spaces)
                Ok(Value::String(" ".repeat(n)))
            }

            // --- Binary/Random File Data Functions ---
            "MKI$" => {
                if args.len() != 1 {
                    return Err("MKI$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as i16;
                let bytes = n.to_le_bytes();
                Ok(Value::String(bytes.iter().map(|&b| b as char).collect()))
            }
            "MKS$" => {
                if args.len() != 1 {
                    return Err("MKS$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()? as f32;
                let bytes = n.to_le_bytes();
                Ok(Value::String(bytes.iter().map(|&b| b as char).collect()))
            }
            "MKD$" => {
                if args.len() != 1 {
                    return Err("MKD$ expects 1 argument".to_string());
                }
                let n = args[0].as_number()?;
                let bytes = n.to_le_bytes();
                Ok(Value::String(bytes.iter().map(|&b| b as char).collect()))
            }
            "CVI" => {
                if args.len() != 1 {
                    return Err("CVI expects 1 argument".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("CVI expects a string argument".to_string()),
                };
                if s.len() < 2 {
                    return Err("CVI requires a 2-byte string".to_string());
                }
                let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
                let n = i16::from_le_bytes([bytes[0], bytes[1]]);
                Ok(Value::Number(n as f64))
            }
            "CVS" => {
                if args.len() != 1 {
                    return Err("CVS expects 1 argument".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("CVS expects a string argument".to_string()),
                };
                if s.len() < 4 {
                    return Err("CVS requires a 4-byte string".to_string());
                }
                let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
                let n = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Ok(Value::Number(n as f64))
            }
            "CVD" => {
                if args.len() != 1 {
                    return Err("CVD expects 1 argument".to_string());
                }
                let s = match &args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err("CVD expects a string argument".to_string()),
                };
                if s.len() < 8 {
                    return Err("CVD requires an 8-byte string".to_string());
                }
                let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
                let n = f64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]);
                Ok(Value::Number(n))
            }

            _ => {
                // Check for user-defined FN functions
                if let Some(func) = self.user_functions.get(name).cloned() {
                    if args.len() != func.params.len() {
                        return Err(format!(
                            "{} expects {} argument(s), got {}",
                            name,
                            func.params.len(),
                            args.len()
                        ));
                    }
                    // Save current values of parameter variables
                    let saved: Vec<(String, Option<Value>)> = func
                        .params
                        .iter()
                        .map(|p| (p.clone(), self.variables.get(p).cloned()))
                        .collect();
                    // Set parameter variables to argument values
                    for (param, arg) in func.params.iter().zip(args.iter()) {
                        self.variables.insert(param.clone(), arg.clone());
                    }
                    // Evaluate the function body
                    let result = self.eval_expr(&func.body);
                    // Restore saved values
                    for (param, saved_val) in saved {
                        match saved_val {
                            Some(v) => {
                                self.variables.insert(param, v);
                            }
                            None => {
                                self.variables.remove(&param);
                            }
                        }
                    }
                    result
                } else {
                    Err(format!("Unknown function: {}", name))
                }
            }
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

    fn eval_err(input: &str) -> String {
        let tokens = Lexer::new(input).tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        evaluator.eval_expr(&expr).unwrap_err()
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

    #[test]
    fn test_value_display_integer() {
        assert_eq!(format!("{}", Value::Number(5.0)), "5");
    }

    #[test]
    fn test_value_display_float() {
        assert_eq!(format!("{}", Value::Number(3.14)), "3.14");
    }

    #[test]
    fn test_value_display_negative_integer() {
        assert_eq!(format!("{}", Value::Number(-7.0)), "-7");
    }

    #[test]
    fn test_value_display_string() {
        assert_eq!(format!("{}", Value::String("HELLO".to_string())), "HELLO");
    }

    #[test]
    fn test_value_print_string_positive_float() {
        assert_eq!(Value::Number(3.14).to_print_string(), " 3.14 ");
    }

    #[test]
    fn test_value_print_string_negative_float() {
        assert_eq!(Value::Number(-3.14).to_print_string(), "-3.14 ");
    }

    #[test]
    fn test_value_print_string_zero() {
        assert_eq!(Value::Number(0.0).to_print_string(), " 0 ");
    }

    #[test]
    fn test_value_as_number_error() {
        let val = Value::String("hello".to_string());
        assert!(val.as_number().is_err());
    }

    #[test]
    fn test_eval_string_not_equal() {
        assert_eq!(eval("\"ABC\" <> \"DEF\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" <> \"ABC\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_string_less() {
        assert_eq!(eval("\"ABC\" < \"DEF\""), Value::Number(-1.0));
        assert_eq!(eval("\"DEF\" < \"ABC\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_string_greater() {
        assert_eq!(eval("\"DEF\" > \"ABC\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" > \"DEF\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_string_less_equal() {
        assert_eq!(eval("\"ABC\" <= \"DEF\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" <= \"ABC\""), Value::Number(-1.0));
        assert_eq!(eval("\"DEF\" <= \"ABC\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_string_greater_equal() {
        assert_eq!(eval("\"DEF\" >= \"ABC\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" >= \"ABC\""), Value::Number(-1.0));
        assert_eq!(eval("\"ABC\" >= \"DEF\""), Value::Number(0.0));
    }

    #[test]
    fn test_eval_string_subtract_error() {
        let tokens = Lexer::new("\"A\" - \"B\"").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_unary_minus_string_error() {
        let tokens = Lexer::new("-X").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        evaluator
            .variables
            .insert("X".to_string(), Value::String("hello".to_string()));
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- Logical operator tests ---

    #[test]
    fn test_eval_and_true_true() {
        // -1 AND -1 = -1 (true AND true = true)
        assert_eq!(eval("(1 = 1) AND (2 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_and_true_false() {
        // -1 AND 0 = 0 (true AND false = false)
        assert_eq!(eval("(1 = 1) AND (1 = 2)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_and_false_false() {
        // 0 AND 0 = 0
        assert_eq!(eval("(1 = 2) AND (3 = 4)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_or_true_false() {
        // -1 OR 0 = -1 (true OR false = true)
        assert_eq!(eval("(1 = 1) OR (1 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_or_false_false() {
        // 0 OR 0 = 0
        assert_eq!(eval("(1 = 2) OR (3 = 4)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_or_true_true() {
        // -1 OR -1 = -1
        assert_eq!(eval("(1 = 1) OR (2 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_xor_true_true() {
        // -1 XOR -1 = 0
        assert_eq!(eval("(1 = 1) XOR (2 = 2)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_xor_true_false() {
        // -1 XOR 0 = -1
        assert_eq!(eval("(1 = 1) XOR (1 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_xor_false_false() {
        // 0 XOR 0 = 0
        assert_eq!(eval("(1 = 2) XOR (3 = 4)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_not_true() {
        // NOT -1 = 0
        assert_eq!(eval("NOT (1 = 1)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_not_false() {
        // NOT 0 = -1
        assert_eq!(eval("NOT (1 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_not_double() {
        // NOT NOT -1 = -1
        assert_eq!(eval("NOT NOT (1 = 1)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_and_with_comparisons() {
        let result = eval_with_vars("X > 0 AND X < 10", vec![("X", Value::Number(5.0))]);
        assert_eq!(result, Value::Number(-1.0));
    }

    #[test]
    fn test_eval_and_with_comparisons_false() {
        let result = eval_with_vars("X > 0 AND X < 10", vec![("X", Value::Number(15.0))]);
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn test_eval_or_with_comparisons() {
        let result = eval_with_vars("X < 0 OR X > 10", vec![("X", Value::Number(15.0))]);
        assert_eq!(result, Value::Number(-1.0));
    }

    #[test]
    fn test_eval_not_with_comparison() {
        let result = eval_with_vars("NOT X = 5", vec![("X", Value::Number(3.0))]);
        assert_eq!(result, Value::Number(-1.0));
    }

    #[test]
    fn test_eval_not_with_comparison_true() {
        let result = eval_with_vars("NOT X = 5", vec![("X", Value::Number(5.0))]);
        assert_eq!(result, Value::Number(0.0));
    }

    #[test]
    fn test_eval_complex_logical() {
        // (X > 0 AND X < 10) OR Y = 0
        let result = eval_with_vars(
            "(X > 0 AND X < 10) OR Y = 0",
            vec![("X", Value::Number(15.0)), ("Y", Value::Number(0.0))],
        );
        assert_eq!(result, Value::Number(-1.0));
    }

    #[test]
    fn test_eval_and_bitwise_integers() {
        // 5 AND 3 = 1 (bitwise: 101 & 011 = 001)
        assert_eq!(eval("5 AND 3"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_or_bitwise_integers() {
        // 5 OR 3 = 7 (bitwise: 101 | 011 = 111)
        assert_eq!(eval("5 OR 3"), Value::Number(7.0));
    }

    #[test]
    fn test_eval_xor_bitwise_integers() {
        // 5 XOR 3 = 6 (bitwise: 101 ^ 011 = 110)
        assert_eq!(eval("5 XOR 3"), Value::Number(6.0));
    }

    #[test]
    fn test_eval_not_zero() {
        // NOT 0 = -1
        assert_eq!(eval("NOT 0"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_not_one() {
        // NOT 1 = -2 (bitwise complement of 1)
        assert_eq!(eval("NOT 1"), Value::Number(-2.0));
    }

    #[test]
    fn test_eval_not_string_error() {
        let tokens = Lexer::new("NOT X").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        evaluator
            .variables
            .insert("X".to_string(), Value::String("hello".to_string()));
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_and_string_error() {
        let tokens = Lexer::new("X AND Y").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        evaluator
            .variables
            .insert("X".to_string(), Value::String("hello".to_string()));
        evaluator.variables.insert("Y".to_string(), Value::Number(1.0));
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_logical_precedence_and_before_or() {
        // 0 OR -1 AND -1 = 0 OR (-1 AND -1) = 0 OR -1 = -1
        // Using comparison results: (1=2) OR (1=1) AND (2=2) = 0 OR (-1 AND -1) = 0 OR -1 = -1
        assert_eq!(eval("(1 = 2) OR (1 = 1) AND (2 = 2)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_logical_with_randomized_and() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let a: i64 = rng.gen_range(-100..100);
            let b: i64 = rng.gen_range(-100..100);
            let input = format!("{} AND {}", a, b);
            let result = eval(&input);
            assert_eq!(result, Value::Number((a & b) as f64));
        }
    }

    #[test]
    fn test_eval_logical_with_randomized_or() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let a: i64 = rng.gen_range(-100..100);
            let b: i64 = rng.gen_range(-100..100);
            let input = if a < 0 && b < 0 {
                format!("({}) OR ({})", a, b)
            } else if a < 0 {
                format!("({}) OR {}", a, b)
            } else if b < 0 {
                format!("{} OR ({})", a, b)
            } else {
                format!("{} OR {}", a, b)
            };
            let result = eval(&input);
            assert_eq!(result, Value::Number((a | b) as f64));
        }
    }

    #[test]
    fn test_eval_logical_with_randomized_xor() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let a: i64 = rng.gen_range(0..100);
            let b: i64 = rng.gen_range(0..100);
            let input = format!("{} XOR {}", a, b);
            let result = eval(&input);
            assert_eq!(result, Value::Number((a ^ b) as f64));
        }
    }

    #[test]
    fn test_eval_logical_with_randomized_not() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let a: i64 = rng.gen_range(0..100);
            let input = format!("NOT {}", a);
            let result = eval(&input);
            assert_eq!(result, Value::Number(!a as f64));
        }
    }

    // --- LEFT$ tests ---

    #[test]
    fn test_eval_left_basic() {
        assert_eq!(eval("LEFT$(\"HELLO\", 3)"), Value::String("HEL".to_string()));
    }

    #[test]
    fn test_eval_left_full_string() {
        assert_eq!(eval("LEFT$(\"HELLO\", 5)"), Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_eval_left_exceeds_length() {
        assert_eq!(eval("LEFT$(\"HI\", 10)"), Value::String("HI".to_string()));
    }

    #[test]
    fn test_eval_left_zero() {
        assert_eq!(eval("LEFT$(\"HELLO\", 0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_left_empty_string() {
        assert_eq!(eval("LEFT$(\"\", 3)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_left_wrong_arg_count() {
        let tokens = Lexer::new("LEFT$(\"HI\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_left_wrong_type() {
        let tokens = Lexer::new("LEFT$(42, 3)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_left_randomized() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let test_strings = ["HELLO", "WORLD", "BASIC", "PROGRAMMING", "A"];
        for _ in 0..20 {
            let s = test_strings[rng.gen_range(0..test_strings.len())];
            let n = rng.gen_range(0..=s.len() + 2);
            let input = format!("LEFT$(\"{}\", {})", s, n);
            let result = eval(&input);
            let expected: String = s.chars().take(n).collect();
            assert_eq!(result, Value::String(expected));
        }
    }

    // --- RIGHT$ tests ---

    #[test]
    fn test_eval_right_basic() {
        assert_eq!(eval("RIGHT$(\"HELLO\", 3)"), Value::String("LLO".to_string()));
    }

    #[test]
    fn test_eval_right_full_string() {
        assert_eq!(eval("RIGHT$(\"HELLO\", 5)"), Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_eval_right_exceeds_length() {
        assert_eq!(eval("RIGHT$(\"HI\", 10)"), Value::String("HI".to_string()));
    }

    #[test]
    fn test_eval_right_zero() {
        assert_eq!(eval("RIGHT$(\"HELLO\", 0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_right_empty_string() {
        assert_eq!(eval("RIGHT$(\"\", 3)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_right_wrong_arg_count() {
        let tokens = Lexer::new("RIGHT$(\"HI\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_right_wrong_type() {
        let tokens = Lexer::new("RIGHT$(42, 3)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- MID$ tests ---

    #[test]
    fn test_eval_mid_three_args() {
        assert_eq!(eval("MID$(\"HELLO\", 2, 3)"), Value::String("ELL".to_string()));
    }

    #[test]
    fn test_eval_mid_two_args() {
        assert_eq!(eval("MID$(\"HELLO\", 2)"), Value::String("ELLO".to_string()));
    }

    #[test]
    fn test_eval_mid_from_start() {
        assert_eq!(eval("MID$(\"HELLO\", 1, 3)"), Value::String("HEL".to_string()));
    }

    #[test]
    fn test_eval_mid_exceeds_length() {
        assert_eq!(eval("MID$(\"HELLO\", 3, 100)"), Value::String("LLO".to_string()));
    }

    #[test]
    fn test_eval_mid_at_end() {
        assert_eq!(eval("MID$(\"HELLO\", 5, 1)"), Value::String("O".to_string()));
    }

    #[test]
    fn test_eval_mid_past_end() {
        assert_eq!(eval("MID$(\"HELLO\", 6)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_mid_zero_length() {
        assert_eq!(eval("MID$(\"HELLO\", 2, 0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_mid_position_zero_error() {
        let tokens = Lexer::new("MID$(\"HELLO\", 0, 3)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_mid_wrong_type() {
        let tokens = Lexer::new("MID$(42, 1, 2)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_mid_wrong_arg_count() {
        let tokens = Lexer::new("MID$(\"HI\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- INSTR tests ---

    #[test]
    fn test_eval_instr_found() {
        assert_eq!(eval("INSTR(\"HELLO WORLD\", \"WORLD\")"), Value::Number(7.0));
    }

    #[test]
    fn test_eval_instr_not_found() {
        assert_eq!(eval("INSTR(\"HELLO\", \"XYZ\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_instr_at_start() {
        assert_eq!(eval("INSTR(\"HELLO\", \"HE\")"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_instr_with_start_pos() {
        assert_eq!(eval("INSTR(7, \"HELLO HELLO\", \"HELLO\")"), Value::Number(7.0));
    }

    #[test]
    fn test_eval_instr_with_start_pos_not_found() {
        assert_eq!(eval("INSTR(6, \"HELLO\", \"HELLO\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_instr_empty_search() {
        assert_eq!(eval("INSTR(\"HELLO\", \"\")"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_instr_empty_string() {
        assert_eq!(eval("INSTR(\"\", \"A\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_instr_wrong_arg_count() {
        let tokens = Lexer::new("INSTR(\"HI\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_instr_start_pos_zero_error() {
        let tokens = Lexer::new("INSTR(0, \"HI\", \"H\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- ASC tests ---

    #[test]
    fn test_eval_asc_basic() {
        assert_eq!(eval("ASC(\"A\")"), Value::Number(65.0));
    }

    #[test]
    fn test_eval_asc_space() {
        assert_eq!(eval("ASC(\" \")"), Value::Number(32.0));
    }

    #[test]
    fn test_eval_asc_multi_char() {
        // ASC returns the code of the first character
        assert_eq!(eval("ASC(\"HELLO\")"), Value::Number(72.0));
    }

    #[test]
    fn test_eval_asc_empty_string_error() {
        let tokens = Lexer::new("ASC(\"\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_asc_wrong_type() {
        let tokens = Lexer::new("ASC(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_asc_wrong_arg_count() {
        let tokens = Lexer::new("ASC()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- CHR$ tests ---

    #[test]
    fn test_eval_chr_basic() {
        assert_eq!(eval("CHR$(65)"), Value::String("A".to_string()));
    }

    #[test]
    fn test_eval_chr_space() {
        assert_eq!(eval("CHR$(32)"), Value::String(" ".to_string()));
    }

    #[test]
    fn test_eval_chr_zero() {
        assert_eq!(eval("CHR$(0)"), Value::String("\0".to_string()));
    }

    #[test]
    fn test_eval_chr_wrong_arg_count() {
        let tokens = Lexer::new("CHR$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_asc_chr_roundtrip() {
        // ASC(CHR$(n)) should return n for printable ASCII
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n = rng.gen_range(32..127);
            let input = format!("ASC(CHR$({}))", n);
            assert_eq!(eval(&input), Value::Number(n as f64));
        }
    }

    // --- STR$ tests ---

    #[test]
    fn test_eval_str_positive_int() {
        assert_eq!(eval("STR$(42)"), Value::String(" 42".to_string()));
    }

    #[test]
    fn test_eval_str_negative_int() {
        assert_eq!(eval("STR$(-5)"), Value::String("-5".to_string()));
    }

    #[test]
    fn test_eval_str_zero() {
        assert_eq!(eval("STR$(0)"), Value::String(" 0".to_string()));
    }

    #[test]
    fn test_eval_str_float() {
        assert_eq!(eval("STR$(3.14)"), Value::String(" 3.14".to_string()));
    }

    #[test]
    fn test_eval_str_wrong_arg_count() {
        let tokens = Lexer::new("STR$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- VAL tests ---

    #[test]
    fn test_eval_val_integer() {
        assert_eq!(eval("VAL(\"42\")"), Value::Number(42.0));
    }

    #[test]
    fn test_eval_val_float() {
        assert_eq!(eval("VAL(\"3.14\")"), Value::Number(3.14));
    }

    #[test]
    fn test_eval_val_negative() {
        assert_eq!(eval("VAL(\"-7\")"), Value::Number(-7.0));
    }

    #[test]
    fn test_eval_val_with_spaces() {
        assert_eq!(eval("VAL(\" 42 \")"), Value::Number(42.0));
    }

    #[test]
    fn test_eval_val_non_numeric() {
        assert_eq!(eval("VAL(\"ABC\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_val_empty_string() {
        assert_eq!(eval("VAL(\"\")"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_val_wrong_type() {
        let tokens = Lexer::new("VAL(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_val_wrong_arg_count() {
        let tokens = Lexer::new("VAL()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- HEX$ tests ---

    #[test]
    fn test_eval_hex_basic() {
        assert_eq!(eval("HEX$(255)"), Value::String("FF".to_string()));
    }

    #[test]
    fn test_eval_hex_zero() {
        assert_eq!(eval("HEX$(0)"), Value::String("0".to_string()));
    }

    #[test]
    fn test_eval_hex_sixteen() {
        assert_eq!(eval("HEX$(16)"), Value::String("10".to_string()));
    }

    #[test]
    fn test_eval_hex_wrong_arg_count() {
        let tokens = Lexer::new("HEX$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_hex_randomized() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n: i64 = rng.gen_range(0..1000);
            let input = format!("HEX$({})", n);
            let result = eval(&input);
            assert_eq!(result, Value::String(format!("{:X}", n)));
        }
    }

    // --- OCT$ tests ---

    #[test]
    fn test_eval_oct_basic() {
        assert_eq!(eval("OCT$(8)"), Value::String("10".to_string()));
    }

    #[test]
    fn test_eval_oct_zero() {
        assert_eq!(eval("OCT$(0)"), Value::String("0".to_string()));
    }

    #[test]
    fn test_eval_oct_255() {
        assert_eq!(eval("OCT$(255)"), Value::String("377".to_string()));
    }

    #[test]
    fn test_eval_oct_wrong_arg_count() {
        let tokens = Lexer::new("OCT$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_oct_randomized() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n: i64 = rng.gen_range(0..1000);
            let input = format!("OCT$({})", n);
            let result = eval(&input);
            assert_eq!(result, Value::String(format!("{:o}", n)));
        }
    }

    // --- STRING$ tests ---

    #[test]
    fn test_eval_string_func_with_string() {
        assert_eq!(eval("STRING$(5, \"*\")"), Value::String("*****".to_string()));
    }

    #[test]
    fn test_eval_string_func_with_number() {
        assert_eq!(eval("STRING$(3, 65)"), Value::String("AAA".to_string()));
    }

    #[test]
    fn test_eval_string_func_zero_length() {
        assert_eq!(eval("STRING$(0, \"X\")"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_string_func_multi_char_uses_first() {
        assert_eq!(eval("STRING$(3, \"HELLO\")"), Value::String("HHH".to_string()));
    }

    #[test]
    fn test_eval_string_func_empty_string_error() {
        let tokens = Lexer::new("STRING$(3, \"\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_string_func_wrong_arg_count() {
        let tokens = Lexer::new("STRING$(3)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- SPACE$ tests ---

    #[test]
    fn test_eval_space_basic() {
        assert_eq!(eval("SPACE$(5)"), Value::String("     ".to_string()));
    }

    #[test]
    fn test_eval_space_zero() {
        assert_eq!(eval("SPACE$(0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_space_one() {
        assert_eq!(eval("SPACE$(1)"), Value::String(" ".to_string()));
    }

    #[test]
    fn test_eval_space_wrong_arg_count() {
        let tokens = Lexer::new("SPACE$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- SPC tests ---

    #[test]
    fn test_eval_spc_basic() {
        assert_eq!(eval("SPC(3)"), Value::String("   ".to_string()));
    }

    #[test]
    fn test_eval_spc_zero() {
        assert_eq!(eval("SPC(0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_spc_wrong_arg_count() {
        let tokens = Lexer::new("SPC()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- TAB tests ---

    #[test]
    fn test_eval_tab_basic() {
        assert_eq!(eval("TAB(10)"), Value::String("          ".to_string()));
    }

    #[test]
    fn test_eval_tab_zero() {
        assert_eq!(eval("TAB(0)"), Value::String("".to_string()));
    }

    #[test]
    fn test_eval_tab_wrong_arg_count() {
        let tokens = Lexer::new("TAB()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- MKI$/CVI roundtrip tests ---

    #[test]
    fn test_eval_mki_cvi_roundtrip() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n: i16 = rng.gen_range(-1000..1000);
            let bytes = n.to_le_bytes();
            let s: String = bytes.iter().map(|&b| b as char).collect();
            let mut evaluator = Evaluator::new();
            evaluator.variables.insert("N".to_string(), Value::Number(n as f64));

            // MKI$(N) -> string
            let tokens = Lexer::new("MKI$(N)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let mki_result = evaluator.eval_expr(&expr).unwrap();
            assert_eq!(mki_result, Value::String(s.clone()));

            // CVI(string) -> N
            evaluator.variables.insert("S$".to_string(), Value::String(s));
            let tokens = Lexer::new("CVI(S$)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let cvi_result = evaluator.eval_expr(&expr).unwrap();
            assert_eq!(cvi_result, Value::Number(n as f64));
        }
    }

    #[test]
    fn test_eval_mki_wrong_arg_count() {
        let tokens = Lexer::new("MKI$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvi_wrong_type() {
        let tokens = Lexer::new("CVI(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvi_too_short() {
        let tokens = Lexer::new("CVI(\"A\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvi_wrong_arg_count() {
        let tokens = Lexer::new("CVI()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- MKS$/CVS tests ---

    #[test]
    fn test_eval_mks_wrong_arg_count() {
        let tokens = Lexer::new("MKS$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvs_wrong_type() {
        let tokens = Lexer::new("CVS(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvs_too_short() {
        let tokens = Lexer::new("CVS(\"AB\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvs_wrong_arg_count() {
        let tokens = Lexer::new("CVS()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- MKD$/CVD tests ---

    #[test]
    fn test_eval_mkd_wrong_arg_count() {
        let tokens = Lexer::new("MKD$()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvd_wrong_type() {
        let tokens = Lexer::new("CVD(42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvd_too_short() {
        let tokens = Lexer::new("CVD(\"ABCD\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_cvd_wrong_arg_count() {
        let tokens = Lexer::new("CVD()").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- Integration tests: string functions composed together ---

    #[test]
    fn test_eval_left_right_compose() {
        // LEFT$(RIGHT$("HELLO WORLD", 5), 3) = LEFT$("WORLD", 3) = "WOR"
        assert_eq!(
            eval("LEFT$(RIGHT$(\"HELLO WORLD\", 5), 3)"),
            Value::String("WOR".to_string())
        );
    }

    #[test]
    fn test_eval_mid_with_instr() {
        // Find "WORLD" in "HELLO WORLD", then extract it
        assert_eq!(
            eval("MID$(\"HELLO WORLD\", INSTR(\"HELLO WORLD\", \"WORLD\"), 5)"),
            Value::String("WORLD".to_string())
        );
    }

    #[test]
    fn test_eval_chr_asc_identity() {
        assert_eq!(eval("CHR$(ASC(\"A\"))"), Value::String("A".to_string()));
    }

    #[test]
    fn test_eval_len_with_space() {
        assert_eq!(eval("LEN(SPACE$(10))"), Value::Number(10.0));
    }

    #[test]
    fn test_eval_val_str_roundtrip() {
        assert_eq!(eval("VAL(STR$(42))"), Value::Number(42.0));
    }

    #[test]
    fn test_eval_string_concatenation_with_chr() {
        assert_eq!(eval("\"A\" + CHR$(66) + \"C\""), Value::String("ABC".to_string()));
    }

    // --- MKS$/CVS roundtrip tests ---

    #[test]
    fn test_eval_mks_cvs_roundtrip() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n: f32 = rng.gen_range(-1000.0..1000.0);
            let bytes = n.to_le_bytes();
            let s: String = bytes.iter().map(|&b| b as char).collect();
            let mut evaluator = Evaluator::new();
            evaluator.variables.insert("N".to_string(), Value::Number(n as f64));

            // MKS$(N) -> string
            let tokens = Lexer::new("MKS$(N)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let mks_result = evaluator.eval_expr(&expr).unwrap();
            assert_eq!(mks_result, Value::String(s.clone()));

            // CVS(string) -> N
            evaluator.variables.insert("S$".to_string(), Value::String(s));
            let tokens = Lexer::new("CVS(S$)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let cvs_result = evaluator.eval_expr(&expr).unwrap();
            if let Value::Number(result) = cvs_result {
                assert!(
                    (result - n as f64).abs() < 0.01,
                    "CVS roundtrip failed: {} vs {}",
                    result,
                    n
                );
            } else {
                panic!("Expected number from CVS");
            }
        }
    }

    // --- MKD$/CVD roundtrip tests ---

    #[test]
    fn test_eval_mkd_cvd_roundtrip() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let n: f64 = rng.gen_range(-1000.0..1000.0);
            let bytes = n.to_le_bytes();
            let s: String = bytes.iter().map(|&b| b as char).collect();
            let mut evaluator = Evaluator::new();
            evaluator.variables.insert("N".to_string(), Value::Number(n));

            // MKD$(N) -> string
            let tokens = Lexer::new("MKD$(N)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let mkd_result = evaluator.eval_expr(&expr).unwrap();
            assert_eq!(mkd_result, Value::String(s.clone()));

            // CVD(string) -> N
            evaluator.variables.insert("S$".to_string(), Value::String(s));
            let tokens = Lexer::new("CVD(S$)").tokenize();
            let mut parser = ExprParser::new(&tokens);
            let expr = parser.parse_expression().unwrap();
            let cvd_result = evaluator.eval_expr(&expr).unwrap();
            assert_eq!(cvd_result, Value::Number(n));
        }
    }

    // --- INSTR error path tests ---

    #[test]
    fn test_eval_instr_two_args_wrong_type_first() {
        let tokens = Lexer::new("INSTR(42, \"A\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_instr_two_args_wrong_type_second() {
        let tokens = Lexer::new("INSTR(\"A\", 42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_instr_three_args_wrong_type_second() {
        let tokens = Lexer::new("INSTR(1, 42, \"A\")").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_instr_three_args_wrong_type_third() {
        let tokens = Lexer::new("INSTR(1, \"A\", 42)").tokenize();
        let mut parser = ExprParser::new(&tokens);
        let expr = parser.parse_expression().unwrap();
        let mut evaluator = Evaluator::new();
        assert!(evaluator.eval_expr(&expr).is_err());
    }

    // --- STR$ negative float test ---

    #[test]
    fn test_eval_str_negative_float() {
        assert_eq!(eval("STR$(-3.14)"), Value::String("-3.14".to_string()));
    }

    // --- EXP function tests ---

    #[test]
    fn test_eval_exp_zero() {
        assert_eq!(eval("EXP(0)"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_exp_one() {
        if let Value::Number(n) = eval("EXP(1)") {
            assert!((n - std::f64::consts::E).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_exp_wrong_arg_count() {
        assert!(eval_err("EXP(1, 2)").contains("EXP expects 1 argument"));
    }

    // --- LOG function tests ---

    #[test]
    fn test_eval_log_one() {
        assert_eq!(eval("LOG(1)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_log_e() {
        if let Value::Number(n) = eval("LOG(2.718281828459045)") {
            assert!((n - 1.0).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_log_negative_error() {
        assert!(eval_err("LOG(-1)").contains("LOG requires a positive argument"));
    }

    #[test]
    fn test_eval_log_zero_error() {
        assert!(eval_err("LOG(0)").contains("LOG requires a positive argument"));
    }

    #[test]
    fn test_eval_log_wrong_arg_count() {
        assert!(eval_err("LOG(1, 2)").contains("LOG expects 1 argument"));
    }

    // --- SGN function tests ---

    #[test]
    fn test_eval_sgn_positive() {
        assert_eq!(eval("SGN(42)"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_sgn_negative() {
        assert_eq!(eval("SGN(-5)"), Value::Number(-1.0));
    }

    #[test]
    fn test_eval_sgn_zero() {
        assert_eq!(eval("SGN(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_sgn_wrong_arg_count() {
        assert!(eval_err("SGN(1, 2)").contains("SGN expects 1 argument"));
    }

    // --- SIN function tests ---

    #[test]
    fn test_eval_sin_zero() {
        assert_eq!(eval("SIN(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_sin_pi_half() {
        if let Value::Number(n) = eval("SIN(1.5707963267948966)") {
            assert!((n - 1.0).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_sin_wrong_arg_count() {
        assert!(eval_err("SIN(1, 2)").contains("SIN expects 1 argument"));
    }

    // --- COS function tests ---

    #[test]
    fn test_eval_cos_zero() {
        assert_eq!(eval("COS(0)"), Value::Number(1.0));
    }

    #[test]
    fn test_eval_cos_pi() {
        if let Value::Number(n) = eval("COS(3.141592653589793)") {
            assert!((n - (-1.0)).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_cos_wrong_arg_count() {
        assert!(eval_err("COS(1, 2)").contains("COS expects 1 argument"));
    }

    // --- TAN function tests ---

    #[test]
    fn test_eval_tan_zero() {
        assert_eq!(eval("TAN(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_tan_wrong_arg_count() {
        assert!(eval_err("TAN(1, 2)").contains("TAN expects 1 argument"));
    }

    // --- ATN function tests ---

    #[test]
    fn test_eval_atn_zero() {
        assert_eq!(eval("ATN(0)"), Value::Number(0.0));
    }

    #[test]
    fn test_eval_atn_one() {
        if let Value::Number(n) = eval("ATN(1)") {
            assert!((n - std::f64::consts::FRAC_PI_4).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_atn_wrong_arg_count() {
        assert!(eval_err("ATN(1, 2)").contains("ATN expects 1 argument"));
    }

    // --- FIX function tests ---

    #[test]
    fn test_eval_fix_positive() {
        assert_eq!(eval("FIX(3.7)"), Value::Number(3.0));
    }

    #[test]
    fn test_eval_fix_negative() {
        assert_eq!(eval("FIX(-3.7)"), Value::Number(-3.0));
    }

    #[test]
    fn test_eval_fix_wrong_arg_count() {
        assert!(eval_err("FIX(1, 2)").contains("FIX expects 1 argument"));
    }

    // --- CINT function tests ---

    #[test]
    fn test_eval_cint_round_up() {
        assert_eq!(eval("CINT(3.6)"), Value::Number(4.0));
    }

    #[test]
    fn test_eval_cint_round_down() {
        assert_eq!(eval("CINT(3.2)"), Value::Number(3.0));
    }

    #[test]
    fn test_eval_cint_negative() {
        assert_eq!(eval("CINT(-3.6)"), Value::Number(-4.0));
    }

    #[test]
    fn test_eval_cint_wrong_arg_count() {
        assert!(eval_err("CINT(1, 2)").contains("CINT expects 1 argument"));
    }

    // --- CSNG function tests ---

    #[test]
    fn test_eval_csng_basic() {
        if let Value::Number(n) = eval("CSNG(3.14)") {
            assert!((n - 3.14).abs() < 0.001);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_eval_csng_wrong_arg_count() {
        assert!(eval_err("CSNG(1, 2)").contains("CSNG expects 1 argument"));
    }

    // --- CDBL function tests ---

    #[test]
    fn test_eval_cdbl_basic() {
        assert_eq!(eval("CDBL(3.14)"), Value::Number(3.14));
    }

    #[test]
    fn test_eval_cdbl_wrong_arg_count() {
        assert!(eval_err("CDBL(1, 2)").contains("CDBL expects 1 argument"));
    }

    // --- EXP/LOG roundtrip ---

    #[test]
    fn test_eval_exp_log_roundtrip() {
        if let Value::Number(n) = eval("LOG(EXP(5))") {
            assert!((n - 5.0).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }

    // --- SIN/ATN identity ---

    #[test]
    fn test_eval_sin_atn_identity() {
        // TAN(ATN(1)) should equal 1
        if let Value::Number(n) = eval("TAN(ATN(1))") {
            assert!((n - 1.0).abs() < 1e-10);
        } else {
            panic!("Expected number");
        }
    }
}
