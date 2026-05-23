//! String built-in functions: LEN, LEFT$, RIGHT$, MID$, INSTR, STRING$, SPACE$,
//! SPC, TAB.
//!
//! Each function takes the pre-evaluated argument slice and returns a `Value`
//! (or an error string). Positions in BASIC are 1-based; the implementations
//! translate to 0-based slicing internally.

use crate::eval::Value;

/// Extracts the underlying string from `value`, or returns an error using
/// `func` as the function name and `which` for the positional description
/// ("first argument", "string argument", etc.).
fn as_string(value: &Value, func: &str, which: &str) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(_) => Err(format!("{} expects a string as {}", func, which)),
    }
}

/// `LEN(s$)` — Returns the length (in bytes) of the string.
pub fn len(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("LEN expects 1 argument".to_string());
    }
    match &args[0] {
        Value::String(s) => Ok(Value::Number(s.len() as f64)),
        _ => Err("LEN expects a string argument".to_string()),
    }
}

/// `LEFT$(s$, n)` — Returns the first `n` characters of `s$`. If `n` exceeds
/// the string's length, the whole string is returned.
pub fn left(args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("LEFT$ expects 2 arguments".to_string());
    }
    let s = as_string(&args[0], "LEFT$", "first argument")?;
    let n = args[1].as_number()? as usize;
    Ok(Value::String(s.chars().take(n).collect()))
}

/// `RIGHT$(s$, n)` — Returns the last `n` characters of `s$`. If `n` exceeds
/// the string's length, the whole string is returned.
pub fn right(args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("RIGHT$ expects 2 arguments".to_string());
    }
    let s = as_string(&args[0], "RIGHT$", "first argument")?;
    let n = args[1].as_number()? as usize;
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    Ok(Value::String(chars[start..].iter().collect()))
}

/// `MID$(s$, n [, m])` — Returns the substring of `s$` starting at 1-based
/// position `n`. If `m` is supplied, at most `m` characters are returned.
/// Errors if `n < 1`.
pub fn mid(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err("MID$ expects 2 or 3 arguments".to_string());
    }
    let s = as_string(&args[0], "MID$", "first argument")?;
    let n = args[1].as_number()? as usize;
    if n == 0 {
        return Err("MID$ position must be >= 1".to_string());
    }
    let chars: Vec<char> = s.chars().collect();
    let start = (n - 1).min(chars.len());
    if args.len() == 3 {
        let m = args[2].as_number()? as usize;
        let end = (start + m).min(chars.len());
        Ok(Value::String(chars[start..end].iter().collect()))
    } else {
        Ok(Value::String(chars[start..].iter().collect()))
    }
}

/// `INSTR([n,] x$, y$)` — Returns the 1-based position of `y$` within `x$`,
/// or 0 if not found. With a 3-argument form, the search starts at position
/// `n` (1-based). Errors if the start position is `< 1`.
pub fn instr(args: &[Value]) -> Result<Value, String> {
    match args.len() {
        2 => {
            let x = as_string(&args[0], "INSTR", "string argument")?;
            let y = as_string(&args[1], "INSTR", "string argument")?;
            Ok(Value::Number(match x.find(&y) {
                Some(pos) => (pos + 1) as f64,
                None => 0.0,
            }))
        }
        3 => {
            let n = args[0].as_number()? as usize;
            if n == 0 {
                return Err("INSTR start position must be >= 1".to_string());
            }
            let x = as_string(&args[1], "INSTR", "string argument")?;
            let y = as_string(&args[2], "INSTR", "string argument")?;
            let start = (n - 1).min(x.len());
            Ok(Value::Number(match x[start..].find(&y) {
                Some(pos) => (pos + start + 1) as f64,
                None => 0.0,
            }))
        }
        _ => Err("INSTR expects 2 or 3 arguments".to_string()),
    }
}

/// `STRING$(n, ch)` — Returns a string of `n` copies of a single character.
/// If `ch` is a string, the first character is used. If `ch` is a number,
/// it's interpreted as an ASCII code. Errors if `ch` is an empty string.
pub fn string(args: &[Value]) -> Result<Value, String> {
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

/// `SPACE$(n)` — Returns a string of `n` spaces.
pub fn space(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("SPACE$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as usize;
    Ok(Value::String(" ".repeat(n)))
}

/// `SPC(n)` — Returns `n` spaces (used inside PRINT statements).
pub fn spc(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("SPC expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as usize;
    Ok(Value::String(" ".repeat(n)))
}

/// `TAB(n)` — Returns spaces to reach column `n` (simplified: just `n` spaces).
pub fn tab(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("TAB expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as usize;
    Ok(Value::String(" ".repeat(n)))
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
    fn len_counts_bytes() {
        assert_eq!(len(&[s("HELLO")]).unwrap(), n(5.0));
        assert_eq!(len(&[s("")]).unwrap(), n(0.0));
    }

    #[test]
    fn len_wrong_arity() {
        assert_eq!(len(&[]).unwrap_err(), "LEN expects 1 argument");
    }

    #[test]
    fn len_rejects_number() {
        assert_eq!(len(&[n(1.0)]).unwrap_err(), "LEN expects a string argument");
    }

    #[test]
    fn left_basic_and_overflow() {
        assert_eq!(left(&[s("HELLO"), n(3.0)]).unwrap(), s("HEL"));
        assert_eq!(left(&[s("HI"), n(99.0)]).unwrap(), s("HI"));
        assert_eq!(left(&[s("HI"), n(0.0)]).unwrap(), s(""));
    }

    #[test]
    fn left_wrong_arity() {
        assert_eq!(left(&[]).unwrap_err(), "LEFT$ expects 2 arguments");
    }

    #[test]
    fn left_rejects_number_first() {
        assert_eq!(
            left(&[n(1.0), n(1.0)]).unwrap_err(),
            "LEFT$ expects a string as first argument"
        );
    }

    #[test]
    fn left_rejects_string_count() {
        assert!(left(&[s("HI"), s("x")]).is_err());
    }

    #[test]
    fn right_basic_and_overflow() {
        assert_eq!(right(&[s("HELLO"), n(3.0)]).unwrap(), s("LLO"));
        assert_eq!(right(&[s("HI"), n(99.0)]).unwrap(), s("HI"));
        assert_eq!(right(&[s("HI"), n(0.0)]).unwrap(), s(""));
    }

    #[test]
    fn right_wrong_arity() {
        assert_eq!(right(&[]).unwrap_err(), "RIGHT$ expects 2 arguments");
    }

    #[test]
    fn right_rejects_number_first() {
        assert_eq!(
            right(&[n(1.0), n(1.0)]).unwrap_err(),
            "RIGHT$ expects a string as first argument"
        );
    }

    #[test]
    fn right_rejects_string_count() {
        assert!(right(&[s("HI"), s("x")]).is_err());
    }

    #[test]
    fn mid_two_arg_form() {
        assert_eq!(mid(&[s("HELLO"), n(2.0)]).unwrap(), s("ELLO"));
        assert_eq!(mid(&[s("HELLO"), n(99.0)]).unwrap(), s(""));
    }

    #[test]
    fn mid_three_arg_form() {
        assert_eq!(mid(&[s("HELLO"), n(2.0), n(3.0)]).unwrap(), s("ELL"));
        assert_eq!(mid(&[s("HELLO"), n(2.0), n(99.0)]).unwrap(), s("ELLO"));
    }

    #[test]
    fn mid_zero_position_errors() {
        assert_eq!(mid(&[s("X"), n(0.0)]).unwrap_err(), "MID$ position must be >= 1");
    }

    #[test]
    fn mid_wrong_arity() {
        assert_eq!(mid(&[s("X")]).unwrap_err(), "MID$ expects 2 or 3 arguments");
        assert_eq!(
            mid(&[s("X"), n(1.0), n(1.0), n(1.0)]).unwrap_err(),
            "MID$ expects 2 or 3 arguments"
        );
    }

    #[test]
    fn mid_rejects_number_first() {
        assert!(mid(&[n(1.0), n(1.0)]).is_err());
    }

    #[test]
    fn mid_rejects_string_count() {
        assert!(mid(&[s("X"), s("x")]).is_err());
        assert!(mid(&[s("X"), n(1.0), s("x")]).is_err());
    }

    #[test]
    fn instr_two_arg_found_and_missing() {
        assert_eq!(instr(&[s("HELLO"), s("LL")]).unwrap(), n(3.0));
        assert_eq!(instr(&[s("HELLO"), s("Z")]).unwrap(), n(0.0));
    }

    #[test]
    fn instr_three_arg_form() {
        assert_eq!(instr(&[n(4.0), s("ABCABC"), s("A")]).unwrap(), n(4.0));
        assert_eq!(instr(&[n(99.0), s("ABCABC"), s("A")]).unwrap(), n(0.0));
    }

    #[test]
    fn instr_zero_start_errors() {
        assert_eq!(
            instr(&[n(0.0), s("X"), s("X")]).unwrap_err(),
            "INSTR start position must be >= 1"
        );
    }

    #[test]
    fn instr_wrong_arity() {
        assert_eq!(instr(&[]).unwrap_err(), "INSTR expects 2 or 3 arguments");
        assert_eq!(
            instr(&[n(1.0), n(2.0), n(3.0), n(4.0)]).unwrap_err(),
            "INSTR expects 2 or 3 arguments"
        );
    }

    #[test]
    fn instr_rejects_non_strings() {
        assert!(instr(&[n(1.0), s("X")]).is_err());
        assert!(instr(&[s("X"), n(1.0)]).is_err());
        assert!(instr(&[n(1.0), n(2.0), s("X")]).is_err());
        assert!(instr(&[n(1.0), s("X"), n(2.0)]).is_err());
        // 3-arg form: string in the start-position slot must surface a numeric error
        assert!(instr(&[s("bad"), s("X"), s("X")]).is_err());
    }

    #[test]
    fn string_with_string_char() {
        assert_eq!(string(&[n(3.0), s("ABC")]).unwrap(), s("AAA"));
    }

    #[test]
    fn string_with_numeric_code() {
        assert_eq!(string(&[n(2.0), n(65.0)]).unwrap(), s("AA"));
    }

    #[test]
    fn string_empty_char_errors() {
        assert_eq!(
            string(&[n(1.0), s("")]).unwrap_err(),
            "Illegal function call: STRING$ with empty string"
        );
    }

    #[test]
    fn string_wrong_arity() {
        assert_eq!(string(&[]).unwrap_err(), "STRING$ expects 2 arguments");
    }

    #[test]
    fn string_rejects_string_count() {
        assert!(string(&[s("x"), s("A")]).is_err());
    }

    #[test]
    fn space_basic() {
        assert_eq!(space(&[n(3.0)]).unwrap(), s("   "));
        assert_eq!(space(&[n(0.0)]).unwrap(), s(""));
    }

    #[test]
    fn space_wrong_arity() {
        assert_eq!(space(&[]).unwrap_err(), "SPACE$ expects 1 argument");
    }

    #[test]
    fn space_rejects_string() {
        assert!(space(&[s("x")]).is_err());
    }

    #[test]
    fn spc_basic() {
        assert_eq!(spc(&[n(2.0)]).unwrap(), s("  "));
    }

    #[test]
    fn spc_wrong_arity() {
        assert_eq!(spc(&[]).unwrap_err(), "SPC expects 1 argument");
    }

    #[test]
    fn spc_rejects_string() {
        assert!(spc(&[s("x")]).is_err());
    }

    #[test]
    fn tab_basic() {
        assert_eq!(tab(&[n(4.0)]).unwrap(), s("    "));
    }

    #[test]
    fn tab_wrong_arity() {
        assert_eq!(tab(&[]).unwrap_err(), "TAB expects 1 argument");
    }

    #[test]
    fn tab_rejects_string() {
        assert!(tab(&[s("x")]).is_err());
    }
}
