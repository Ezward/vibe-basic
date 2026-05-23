//! Built-in BASIC functions, split into focused submodules.
//!
//! - [`math`] — numeric / trigonometric / conversion-of-precision functions
//!   (INT, ABS, SQR, RND, EXP, LOG, SGN, SIN, COS, TAN, ATN, FIX, CINT, CSNG, CDBL).
//! - [`string`] — substring, search, and string-construction functions
//!   (LEN, LEFT$, RIGHT$, MID$, INSTR, STRING$, SPACE$, SPC, TAB).
//! - [`conversion`] — number↔string and base-conversion functions
//!   (ASC, CHR$, STR$, VAL, HEX$, OCT$).
//! - [`data`] — random-file binary packing/unpacking functions
//!   (MKI$, MKS$, MKD$, CVI, CVS, CVD).

pub mod conversion;
pub mod data;
pub mod math;
pub mod string;
