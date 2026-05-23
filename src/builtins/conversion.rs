//! Conversion built-in functions: ASC, CHR$, STR$, VAL, HEX$, OCT$.
//!
//! These functions convert between numbers, strings, and character codes,
//! following GW-BASIC / MS-BASIC formatting conventions (notably: `STR$` of
//! a non-negative number is prefixed with a leading space).

use crate::eval::Value;

/// `ASC(s$)` — Returns the ASCII code of the first byte of `s$`. Errors if
/// `s$` is empty.
pub fn asc(args: &[Value]) -> Result<Value, String> {
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

/// `CHR$(n)` — Returns a one-character string for ASCII code `n` (low 8 bits).
pub fn chr(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("CHR$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as u8;
    Ok(Value::String(String::from(n as char)))
}

/// `STR$(n)` — Converts `n` to its string representation. Non-negative numbers
/// receive a leading space (GW-BASIC convention). Integer-valued floats are
/// rendered without a trailing decimal.
pub fn str_(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("STR$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()?;
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

/// `VAL(s$)` — Parses `s$` as a number (after trimming whitespace). Returns 0
/// if the string does not parse as a number.
pub fn val(args: &[Value]) -> Result<Value, String> {
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

/// `HEX$(n)` — Returns the uppercase hexadecimal representation of `n`
/// (interpreted as a signed 64-bit integer).
pub fn hex(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("HEX$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as i64;
    Ok(Value::String(format!("{:X}", n)))
}

/// `OCT$(n)` — Returns the octal representation of `n` (interpreted as a
/// signed 64-bit integer).
pub fn oct(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("OCT$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as i64;
    Ok(Value::String(format!("{:o}", n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(x: f64) -> Value {
        Value::Number(x)
    }
    fn s(x: &str) -> Value {
        Value::String(x.to_string())
    }

    #[test]
    fn asc_returns_first_byte() {
        assert_eq!(asc(&[s("A")]).unwrap(), n(65.0));
        assert_eq!(asc(&[s("ABC")]).unwrap(), n(65.0));
    }

    #[test]
    fn asc_empty_errors() {
        assert_eq!(asc(&[s("")]).unwrap_err(), "Illegal function call: ASC of empty string");
    }

    #[test]
    fn asc_wrong_arity() {
        assert_eq!(asc(&[]).unwrap_err(), "ASC expects 1 argument");
    }

    #[test]
    fn asc_rejects_number() {
        assert_eq!(asc(&[n(1.0)]).unwrap_err(), "ASC expects a string argument");
    }

    #[test]
    fn chr_returns_single_char() {
        assert_eq!(chr(&[n(65.0)]).unwrap(), s("A"));
        assert_eq!(chr(&[n(48.0)]).unwrap(), s("0"));
    }

    #[test]
    fn chr_wrong_arity() {
        assert_eq!(chr(&[]).unwrap_err(), "CHR$ expects 1 argument");
    }

    #[test]
    fn chr_rejects_string() {
        assert!(chr(&[s("x")]).is_err());
    }

    #[test]
    fn str_formats_positive_with_leading_space() {
        assert_eq!(str_(&[n(3.0)]).unwrap(), s(" 3"));
        assert_eq!(str_(&[n(0.0)]).unwrap(), s(" 0"));
    }

    #[test]
    fn str_formats_positive_float() {
        assert_eq!(str_(&[n(1.5)]).unwrap(), s(" 1.5"));
    }

    #[test]
    fn str_formats_negative_integer() {
        assert_eq!(str_(&[n(-3.0)]).unwrap(), s("-3"));
    }

    #[test]
    fn str_formats_negative_float() {
        assert_eq!(str_(&[n(-1.5)]).unwrap(), s("-1.5"));
    }

    #[test]
    fn str_wrong_arity() {
        assert_eq!(str_(&[]).unwrap_err(), "STR$ expects 1 argument");
    }

    #[test]
    fn str_rejects_string() {
        assert!(str_(&[s("x")]).is_err());
    }

    #[test]
    fn val_parses_numbers() {
        assert_eq!(val(&[s("42")]).unwrap(), n(42.0));
        assert_eq!(val(&[s(" -3.5 ")]).unwrap(), n(-3.5));
    }

    #[test]
    fn val_unparsable_returns_zero() {
        assert_eq!(val(&[s("HELLO")]).unwrap(), n(0.0));
        assert_eq!(val(&[s("")]).unwrap(), n(0.0));
    }

    #[test]
    fn val_wrong_arity() {
        assert_eq!(val(&[]).unwrap_err(), "VAL expects 1 argument");
    }

    #[test]
    fn val_rejects_number() {
        assert_eq!(val(&[n(1.0)]).unwrap_err(), "VAL expects a string argument");
    }

    #[test]
    fn hex_basic_and_signed() {
        assert_eq!(hex(&[n(255.0)]).unwrap(), s("FF"));
        assert_eq!(hex(&[n(0.0)]).unwrap(), s("0"));
    }

    #[test]
    fn hex_wrong_arity() {
        assert_eq!(hex(&[]).unwrap_err(), "HEX$ expects 1 argument");
    }

    #[test]
    fn hex_rejects_string() {
        assert!(hex(&[s("x")]).is_err());
    }

    #[test]
    fn oct_basic() {
        assert_eq!(oct(&[n(8.0)]).unwrap(), s("10"));
        assert_eq!(oct(&[n(0.0)]).unwrap(), s("0"));
    }

    #[test]
    fn oct_wrong_arity() {
        assert_eq!(oct(&[]).unwrap_err(), "OCT$ expects 1 argument");
    }

    #[test]
    fn oct_rejects_string() {
        assert!(oct(&[s("x")]).is_err());
    }
}
