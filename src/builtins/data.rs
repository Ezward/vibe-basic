//! Binary data built-in functions used for random-file I/O:
//! MKI$, MKS$, MKD$ (number → little-endian byte string) and
//! CVI, CVS, CVD (little-endian byte string → number).
//!
//! Each "MK" function packs a number into a fixed-width little-endian byte
//! string, and each "CV" function unpacks one. The strings are treated as
//! sequences of single-byte characters (each `char` encodes one byte).

use crate::eval::Value;

/// `MKI$(x)` — Packs `x` as a 2-byte little-endian signed integer string.
pub fn mki(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("MKI$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as i16;
    Ok(Value::String(n.to_le_bytes().iter().map(|&b| b as char).collect()))
}

/// `MKS$(x)` — Packs `x` as a 4-byte little-endian single-precision string.
pub fn mks(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("MKS$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()? as f32;
    Ok(Value::String(n.to_le_bytes().iter().map(|&b| b as char).collect()))
}

/// `MKD$(x)` — Packs `x` as an 8-byte little-endian double-precision string.
pub fn mkd(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("MKD$ expects 1 argument".to_string());
    }
    let n = args[0].as_number()?;
    Ok(Value::String(n.to_le_bytes().iter().map(|&b| b as char).collect()))
}

/// Extracts the underlying string from `args[0]` or returns an "expects a
/// string argument" error tagged with `func`.
fn arg_string(args: &[Value], func: &str) -> Result<String, String> {
    match &args[0] {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("{} expects a string argument", func)),
    }
}

/// `CVI(s$)` — Unpacks the first 2 bytes of `s$` as a little-endian signed
/// 16-bit integer. Errors if the string is shorter than 2 bytes.
pub fn cvi(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("CVI expects 1 argument".to_string());
    }
    let s = arg_string(args, "CVI")?;
    if s.len() < 2 {
        return Err("CVI requires a 2-byte string".to_string());
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
    Ok(Value::Number(i16::from_le_bytes([bytes[0], bytes[1]]) as f64))
}

/// `CVS(s$)` — Unpacks the first 4 bytes of `s$` as a little-endian single
/// precision float. Errors if the string is shorter than 4 bytes.
pub fn cvs(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("CVS expects 1 argument".to_string());
    }
    let s = arg_string(args, "CVS")?;
    if s.len() < 4 {
        return Err("CVS requires a 4-byte string".to_string());
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
    Ok(Value::Number(
        f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64,
    ))
}

/// `CVD(s$)` — Unpacks the first 8 bytes of `s$` as a little-endian double
/// precision float. Errors if the string is shorter than 8 bytes.
pub fn cvd(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("CVD expects 1 argument".to_string());
    }
    let s = arg_string(args, "CVD")?;
    if s.len() < 8 {
        return Err("CVD requires an 8-byte string".to_string());
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
    Ok(Value::Number(f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])))
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
    fn mki_then_cvi_roundtrip() {
        let packed = mki(&[n(258.0)]).unwrap();
        assert_eq!(cvi(&[packed]).unwrap(), n(258.0));
    }

    #[test]
    fn mks_then_cvs_roundtrip() {
        let packed = mks(&[n(1.5)]).unwrap();
        assert_eq!(cvs(&[packed]).unwrap(), n(1.5));
    }

    #[test]
    fn mkd_then_cvd_roundtrip() {
        let packed = mkd(&[n(std::f64::consts::PI)]).unwrap();
        assert_eq!(cvd(&[packed]).unwrap(), n(std::f64::consts::PI));
    }

    #[test]
    fn mki_wrong_arity() {
        assert_eq!(mki(&[]).unwrap_err(), "MKI$ expects 1 argument");
    }

    #[test]
    fn mks_wrong_arity() {
        assert_eq!(mks(&[]).unwrap_err(), "MKS$ expects 1 argument");
    }

    #[test]
    fn mkd_wrong_arity() {
        assert_eq!(mkd(&[]).unwrap_err(), "MKD$ expects 1 argument");
    }

    #[test]
    fn mk_reject_strings() {
        assert!(mki(&[s("x")]).is_err());
        assert!(mks(&[s("x")]).is_err());
        assert!(mkd(&[s("x")]).is_err());
    }

    #[test]
    fn cvi_wrong_arity() {
        assert_eq!(cvi(&[]).unwrap_err(), "CVI expects 1 argument");
    }

    #[test]
    fn cvi_short_string_errors() {
        assert_eq!(cvi(&[s("X")]).unwrap_err(), "CVI requires a 2-byte string");
    }

    #[test]
    fn cvi_rejects_number() {
        assert_eq!(cvi(&[n(1.0)]).unwrap_err(), "CVI expects a string argument");
    }

    #[test]
    fn cvs_wrong_arity() {
        assert_eq!(cvs(&[]).unwrap_err(), "CVS expects 1 argument");
    }

    #[test]
    fn cvs_short_string_errors() {
        assert_eq!(cvs(&[s("XYZ")]).unwrap_err(), "CVS requires a 4-byte string");
    }

    #[test]
    fn cvs_rejects_number() {
        assert_eq!(cvs(&[n(1.0)]).unwrap_err(), "CVS expects a string argument");
    }

    #[test]
    fn cvd_wrong_arity() {
        assert_eq!(cvd(&[]).unwrap_err(), "CVD expects 1 argument");
    }

    #[test]
    fn cvd_short_string_errors() {
        assert_eq!(cvd(&[s("ABCDEFG")]).unwrap_err(), "CVD requires an 8-byte string");
    }

    #[test]
    fn cvd_rejects_number() {
        assert_eq!(cvd(&[n(1.0)]).unwrap_err(), "CVD expects a string argument");
    }
}
