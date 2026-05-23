//! Numeric built-in functions: INT, ABS, SQR, RND, EXP, LOG, SGN, SIN, COS, TAN,
//! ATN, FIX, CINT, CSNG, CDBL.
//!
//! Each function takes the pre-evaluated argument slice and returns a `Value`
//! (or an error string compatible with the rest of the evaluator). `RND` also
//! takes a mutable reference to a random-number generator, since it needs
//! evaluator-owned state.

use crate::eval::Value;
use rand::Rng;

/// Validates that `args` contains exactly one argument for the named function.
fn expect_one(name: &str, args: &[Value]) -> Result<(), String> {
    if args.len() != 1 {
        Err(format!("{} expects 1 argument", name))
    } else {
        Ok(())
    }
}

/// `INT(x)` — Returns the largest integer less than or equal to `x` (floor).
pub fn int(args: &[Value]) -> Result<Value, String> {
    expect_one("INT", args)?;
    Ok(Value::Number(args[0].as_number()?.floor()))
}

/// `ABS(x)` — Returns the absolute value of `x`.
pub fn abs(args: &[Value]) -> Result<Value, String> {
    expect_one("ABS", args)?;
    Ok(Value::Number(args[0].as_number()?.abs()))
}

/// `SQR(x)` — Returns the square root of `x`. Negative inputs yield `NaN`,
/// matching `f64::sqrt`.
pub fn sqr(args: &[Value]) -> Result<Value, String> {
    expect_one("SQR", args)?;
    Ok(Value::Number(args[0].as_number()?.sqrt()))
}

/// `RND(x)` — Returns a uniformly distributed random float in `[0.0, 1.0)`.
/// The argument is required (MS-BASIC compatibility) but its value is ignored.
pub fn rnd<R: Rng + ?Sized>(args: &[Value], rng: &mut R) -> Result<Value, String> {
    expect_one("RND", args)?;
    let _ignored = args[0].as_number()?;
    let val: f64 = rng.gen();
    Ok(Value::Number(val))
}

/// `EXP(x)` — Returns e raised to the power `x`.
pub fn exp(args: &[Value]) -> Result<Value, String> {
    expect_one("EXP", args)?;
    Ok(Value::Number(args[0].as_number()?.exp()))
}

/// `LOG(x)` — Returns the natural logarithm of `x`. Errors if `x <= 0`.
pub fn log(args: &[Value]) -> Result<Value, String> {
    expect_one("LOG", args)?;
    let x = args[0].as_number()?;
    if x <= 0.0 {
        return Err("LOG requires a positive argument".to_string());
    }
    Ok(Value::Number(x.ln()))
}

/// `SGN(x)` — Returns `1` if `x > 0`, `-1` if `x < 0`, `0` otherwise.
pub fn sgn(args: &[Value]) -> Result<Value, String> {
    expect_one("SGN", args)?;
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

/// `SIN(x)` — Returns the sine of `x` (radians).
pub fn sin(args: &[Value]) -> Result<Value, String> {
    expect_one("SIN", args)?;
    Ok(Value::Number(args[0].as_number()?.sin()))
}

/// `COS(x)` — Returns the cosine of `x` (radians).
pub fn cos(args: &[Value]) -> Result<Value, String> {
    expect_one("COS", args)?;
    Ok(Value::Number(args[0].as_number()?.cos()))
}

/// `TAN(x)` — Returns the tangent of `x` (radians).
pub fn tan(args: &[Value]) -> Result<Value, String> {
    expect_one("TAN", args)?;
    Ok(Value::Number(args[0].as_number()?.tan()))
}

/// `ATN(x)` — Returns the arctangent of `x` (radians, range `(-π/2, π/2)`).
pub fn atn(args: &[Value]) -> Result<Value, String> {
    expect_one("ATN", args)?;
    Ok(Value::Number(args[0].as_number()?.atan()))
}

/// `FIX(x)` — Truncates `x` toward zero (unlike `INT`, which floors).
pub fn fix(args: &[Value]) -> Result<Value, String> {
    expect_one("FIX", args)?;
    Ok(Value::Number(args[0].as_number()?.trunc()))
}

/// `CINT(x)` — Rounds `x` to the nearest integer (half away from zero).
pub fn cint(args: &[Value]) -> Result<Value, String> {
    expect_one("CINT", args)?;
    Ok(Value::Number(args[0].as_number()?.round()))
}

/// `CSNG(x)` — Converts `x` to single precision (round-trip through `f32`).
pub fn csng(args: &[Value]) -> Result<Value, String> {
    expect_one("CSNG", args)?;
    let x = args[0].as_number()?;
    Ok(Value::Number((x as f32) as f64))
}

/// `CDBL(x)` — Converts `x` to double precision. Since all numbers are already
/// stored as `f64`, this is the identity.
pub fn cdbl(args: &[Value]) -> Result<Value, String> {
    expect_one("CDBL", args)?;
    Ok(Value::Number(args[0].as_number()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn n(x: f64) -> Value {
        Value::Number(x)
    }
    fn s(x: &str) -> Value {
        Value::String(x.to_string())
    }

    #[test]
    fn int_floors_positive() {
        assert_eq!(int(&[n(3.7)]).unwrap(), n(3.0));
    }

    #[test]
    fn int_floors_negative() {
        assert_eq!(int(&[n(-2.3)]).unwrap(), n(-3.0));
    }

    #[test]
    fn int_wrong_arity() {
        assert_eq!(int(&[]).unwrap_err(), "INT expects 1 argument");
        assert_eq!(int(&[n(1.0), n(2.0)]).unwrap_err(), "INT expects 1 argument");
    }

    #[test]
    fn int_rejects_string() {
        assert!(int(&[s("x")]).is_err());
    }

    #[test]
    fn abs_handles_negative_and_positive() {
        assert_eq!(abs(&[n(-5.5)]).unwrap(), n(5.5));
        assert_eq!(abs(&[n(0.0)]).unwrap(), n(0.0));
        assert_eq!(abs(&[n(3.0)]).unwrap(), n(3.0));
    }

    #[test]
    fn abs_wrong_arity() {
        assert_eq!(abs(&[]).unwrap_err(), "ABS expects 1 argument");
    }

    #[test]
    fn abs_rejects_string() {
        assert!(abs(&[s("x")]).is_err());
    }

    #[test]
    fn sqr_basic_and_zero() {
        assert_eq!(sqr(&[n(9.0)]).unwrap(), n(3.0));
        assert_eq!(sqr(&[n(0.0)]).unwrap(), n(0.0));
    }

    #[test]
    fn sqr_wrong_arity() {
        assert_eq!(sqr(&[]).unwrap_err(), "SQR expects 1 argument");
    }

    #[test]
    fn sqr_rejects_string() {
        assert!(sqr(&[s("x")]).is_err());
    }

    #[test]
    fn rnd_in_range_and_deterministic_with_seed() {
        let mut rng = StdRng::seed_from_u64(42);
        let v = rnd(&[n(1.0)], &mut rng).unwrap().as_number().unwrap();
        assert!((0.0..1.0).contains(&v));
    }

    #[test]
    fn rnd_wrong_arity() {
        let mut rng = StdRng::seed_from_u64(0);
        assert_eq!(rnd(&[], &mut rng).unwrap_err(), "RND expects 1 argument");
    }

    #[test]
    fn rnd_rejects_string_arg() {
        let mut rng = StdRng::seed_from_u64(0);
        assert!(rnd(&[s("x")], &mut rng).is_err());
    }

    #[test]
    fn exp_at_zero_and_one() {
        assert_eq!(exp(&[n(0.0)]).unwrap(), n(1.0));
        let e = exp(&[n(1.0)]).unwrap().as_number().unwrap();
        assert!((e - std::f64::consts::E).abs() < 1e-12);
    }

    #[test]
    fn exp_wrong_arity() {
        assert_eq!(exp(&[]).unwrap_err(), "EXP expects 1 argument");
    }

    #[test]
    fn exp_rejects_string() {
        assert!(exp(&[s("x")]).is_err());
    }

    #[test]
    fn log_of_one_is_zero() {
        assert_eq!(log(&[n(1.0)]).unwrap(), n(0.0));
    }

    #[test]
    fn log_of_nonpositive_errors() {
        assert_eq!(log(&[n(0.0)]).unwrap_err(), "LOG requires a positive argument");
        assert_eq!(log(&[n(-1.0)]).unwrap_err(), "LOG requires a positive argument");
    }

    #[test]
    fn log_wrong_arity() {
        assert_eq!(log(&[]).unwrap_err(), "LOG expects 1 argument");
    }

    #[test]
    fn log_rejects_string() {
        assert!(log(&[s("x")]).is_err());
    }

    #[test]
    fn sgn_all_three_branches() {
        assert_eq!(sgn(&[n(5.0)]).unwrap(), n(1.0));
        assert_eq!(sgn(&[n(-5.0)]).unwrap(), n(-1.0));
        assert_eq!(sgn(&[n(0.0)]).unwrap(), n(0.0));
    }

    #[test]
    fn sgn_wrong_arity() {
        assert_eq!(sgn(&[]).unwrap_err(), "SGN expects 1 argument");
    }

    #[test]
    fn sgn_rejects_string() {
        assert!(sgn(&[s("x")]).is_err());
    }

    #[test]
    fn trig_functions_basic_values() {
        assert_eq!(sin(&[n(0.0)]).unwrap(), n(0.0));
        assert_eq!(cos(&[n(0.0)]).unwrap(), n(1.0));
        assert_eq!(tan(&[n(0.0)]).unwrap(), n(0.0));
        assert_eq!(atn(&[n(0.0)]).unwrap(), n(0.0));
    }

    #[test]
    fn trig_wrong_arity() {
        assert_eq!(sin(&[]).unwrap_err(), "SIN expects 1 argument");
        assert_eq!(cos(&[]).unwrap_err(), "COS expects 1 argument");
        assert_eq!(tan(&[]).unwrap_err(), "TAN expects 1 argument");
        assert_eq!(atn(&[]).unwrap_err(), "ATN expects 1 argument");
    }

    #[test]
    fn trig_rejects_string() {
        assert!(sin(&[s("x")]).is_err());
        assert!(cos(&[s("x")]).is_err());
        assert!(tan(&[s("x")]).is_err());
        assert!(atn(&[s("x")]).is_err());
    }

    #[test]
    fn fix_truncates_toward_zero() {
        assert_eq!(fix(&[n(3.7)]).unwrap(), n(3.0));
        assert_eq!(fix(&[n(-3.7)]).unwrap(), n(-3.0));
    }

    #[test]
    fn fix_wrong_arity() {
        assert_eq!(fix(&[]).unwrap_err(), "FIX expects 1 argument");
    }

    #[test]
    fn fix_rejects_string() {
        assert!(fix(&[s("x")]).is_err());
    }

    #[test]
    fn cint_rounds_to_nearest() {
        assert_eq!(cint(&[n(2.5)]).unwrap(), n(3.0));
        assert_eq!(cint(&[n(2.4)]).unwrap(), n(2.0));
        assert_eq!(cint(&[n(-2.5)]).unwrap(), n(-3.0));
    }

    #[test]
    fn cint_wrong_arity() {
        assert_eq!(cint(&[]).unwrap_err(), "CINT expects 1 argument");
    }

    #[test]
    fn cint_rejects_string() {
        assert!(cint(&[s("x")]).is_err());
    }

    #[test]
    fn csng_truncates_to_f32_precision() {
        let v = csng(&[n(std::f64::consts::PI)]).unwrap().as_number().unwrap();
        assert_eq!(v, std::f64::consts::PI as f32 as f64);
    }

    #[test]
    fn csng_wrong_arity() {
        assert_eq!(csng(&[]).unwrap_err(), "CSNG expects 1 argument");
    }

    #[test]
    fn csng_rejects_string() {
        assert!(csng(&[s("x")]).is_err());
    }

    #[test]
    fn cdbl_is_identity() {
        assert_eq!(cdbl(&[n(1.234_567)]).unwrap(), n(1.234_567));
    }

    #[test]
    fn cdbl_wrong_arity() {
        assert_eq!(cdbl(&[]).unwrap_err(), "CDBL expects 1 argument");
    }

    #[test]
    fn cdbl_rejects_string() {
        assert!(cdbl(&[s("x")]).is_err());
    }
}
