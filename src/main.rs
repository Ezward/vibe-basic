//! Entry point for the Vibe Basic interpreter.
//!
//! This module reads a BASIC source file, tokenizes it, parses it into an AST,
//! and executes it through the interpreter. The pipeline is:
//! source text -> Lexer (tokens) -> Parser (AST) -> Interpreter (execution).

mod ast;
mod debugger;
mod eval;
mod expr;
mod interpreter;
mod token;

use std::env;
use std::fs;
use std::io;

/// Returns the version string for the application, derived from the Cargo package version.
fn version_string() -> String {
    format!("vibe-basic {}", env!("CARGO_PKG_VERSION"))
}

/// Result of parsing command-line arguments.
#[derive(Debug, PartialEq)]
enum ParsedArgs {
    /// User requested `--version` output.
    Version,
    /// User wants to run a BASIC program, optionally in debug mode.
    Run { filename: String, debug: bool },
}

/// Parses command-line arguments (excluding the program name) into a `ParsedArgs` value.
/// Returns `Err` with a usage message if the arguments are invalid.
fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut debug_mode = false;
    let mut filename = None;

    for arg in args {
        if arg == "--version" {
            return Ok(ParsedArgs::Version);
        } else if arg == "--debug" {
            debug_mode = true;
        } else if filename.is_none() {
            filename = Some(arg.clone());
        } else {
            return Err("Usage: vibe_basic [--version] [--debug] <filename.bas>".to_string());
        }
    }

    match filename {
        Some(f) => Ok(ParsedArgs::Run {
            filename: f,
            debug: debug_mode,
        }),
        None => Err("Usage: vibe_basic [--version] [--debug] <filename.bas>".to_string()),
    }
}

/// Runs the BASIC interpreter: reads the source file specified as a command-line
/// argument, tokenizes and parses it, then executes the resulting program.
/// With `--version`, prints the version and exits.
/// With `--debug`, launches an interactive debugger instead.
fn main() {
    let args: Vec<String> = env::args().collect();

    match parse_args(&args[1..]) {
        Ok(ParsedArgs::Version) => {
            println!("{}", version_string());
        }
        Ok(ParsedArgs::Run { filename, debug }) => {
            let source = match fs::read_to_string(&filename) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file '{}': {}", filename, e);
                    std::process::exit(1);
                }
            };

            let tokens = token::Lexer::new(&source).tokenize();
            let source_lines: Vec<String> = source.lines().map(String::from).collect();
            let mut parser = ast::Parser::new(&tokens, source_lines);
            let program = match parser.parse_program() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    std::process::exit(1);
                }
            };

            let stdin = io::stdin();
            let stdout = io::stdout();

            if debug {
                let interp = interpreter::Interpreter::new(stdin.lock(), stdout.lock());
                let mut dbg = debugger::Debugger::new(interp);
                if let Err(e) = dbg.run_repl(&program) {
                    eprintln!("Debugger error: {}", e);
                    std::process::exit(1);
                }
            } else {
                let mut interp = interpreter::Interpreter::new(stdin.lock(), stdout.lock());
                if let Err(e) = interp.run(&program) {
                    eprintln!("Runtime error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that `--version` is recognized and returns `ParsedArgs::Version`.
    #[test]
    fn test_parse_args_version() {
        let args = vec!["--version".to_string()];
        assert_eq!(parse_args(&args), Ok(ParsedArgs::Version));
    }

    /// Tests that `--version` takes priority even when other arguments are present.
    #[test]
    fn test_parse_args_version_with_other_args() {
        let args = vec!["--version".to_string(), "file.bas".to_string()];
        assert_eq!(parse_args(&args), Ok(ParsedArgs::Version));
    }

    /// Tests that `--version` is recognized after `--debug`.
    #[test]
    fn test_parse_args_version_after_debug() {
        let args = vec!["--debug".to_string(), "--version".to_string()];
        assert_eq!(parse_args(&args), Ok(ParsedArgs::Version));
    }

    /// Tests that a filename alone produces a non-debug run.
    #[test]
    fn test_parse_args_filename_only() {
        let args = vec!["hello.bas".to_string()];
        assert_eq!(
            parse_args(&args),
            Ok(ParsedArgs::Run {
                filename: "hello.bas".to_string(),
                debug: false,
            })
        );
    }

    /// Tests that `--debug` with a filename produces a debug run.
    #[test]
    fn test_parse_args_debug_mode() {
        let args = vec!["--debug".to_string(), "hello.bas".to_string()];
        assert_eq!(
            parse_args(&args),
            Ok(ParsedArgs::Run {
                filename: "hello.bas".to_string(),
                debug: true,
            })
        );
    }

    /// Tests that `--debug` after the filename also works.
    #[test]
    fn test_parse_args_debug_after_filename() {
        let args = vec!["hello.bas".to_string(), "--debug".to_string()];
        assert_eq!(
            parse_args(&args),
            Ok(ParsedArgs::Run {
                filename: "hello.bas".to_string(),
                debug: true,
            })
        );
    }

    /// Tests that no arguments produces an error.
    #[test]
    fn test_parse_args_no_args() {
        let args: Vec<String> = vec![];
        assert!(parse_args(&args).is_err());
    }

    /// Tests that multiple filenames produce an error.
    #[test]
    fn test_parse_args_multiple_filenames() {
        let args = vec!["a.bas".to_string(), "b.bas".to_string()];
        assert!(parse_args(&args).is_err());
    }

    /// Tests that the version string contains the package name and version from Cargo.toml.
    #[test]
    fn test_version_string() {
        let v = version_string();
        assert!(v.starts_with("vibe-basic "));
        assert_eq!(v, format!("vibe-basic {}", env!("CARGO_PKG_VERSION")));
    }

    /// Tests version string with a randomized check to ensure it's not empty.
    #[test]
    fn test_version_string_not_empty() {
        let v = version_string();
        assert!(!v.is_empty());
        // The version portion should contain at least one digit
        let version_part = v.strip_prefix("vibe-basic ").unwrap();
        assert!(version_part.chars().any(|c| c.is_ascii_digit()));
    }
}
