//! Entry point for the Vibe Basic interpreter.
//!
//! This module reads a BASIC source file, tokenizes it, parses it into an AST,
//! and executes it through the interpreter. The pipeline is:
//! source text -> Lexer (tokens) -> Parser (AST) -> Interpreter (execution).

mod ast;
mod builtins;
mod debugger;
mod eval;
mod expr;
mod interpreter;
mod token;

use std::env;
use std::fs;
use std::io::{self, BufReader};
use std::net::TcpListener;

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
    Run {
        filename: String,
        debug: bool,
        debug_port: Option<u16>,
    },
}

/// Parses command-line arguments (excluding the program name) into a `ParsedArgs` value.
/// Returns `Err` with a usage message if the arguments are invalid.
fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let usage = "Usage: vibe-basic [--version] [--debug [--debug-port <port>]] <filename.bas>";
    let mut debug_mode = false;
    let mut debug_port: Option<u16> = None;
    let mut filename = None;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if arg == "--version" {
            return Ok(ParsedArgs::Version);
        } else if arg == "--debug" {
            debug_mode = true;
        } else if arg == "--debug-port" {
            i += 1;
            if i >= args.len() {
                return Err(format!("--debug-port requires a port number\n{}", usage));
            }
            debug_port = Some(
                args[i]
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port number: {}\n{}", args[i], usage))?,
            );
        } else if filename.is_none() {
            filename = Some(arg.clone());
        } else {
            return Err(usage.to_string());
        }
        i += 1;
    }

    if debug_port.is_some() && !debug_mode {
        return Err(format!("--debug-port requires --debug\n{}", usage));
    }

    match filename {
        Some(f) => Ok(ParsedArgs::Run {
            filename: f,
            debug: debug_mode,
            debug_port,
        }),
        None => Err(usage.to_string()),
    }
}

/// Runs the BASIC interpreter: reads the source file specified as a command-line
/// argument, tokenizes and parses it, then executes the resulting program.
/// With `--version`, prints the version and exits.
/// With `--debug`, launches an interactive debugger instead.
/// With `--debug --debug-port <port>`, the debugger listens for a TCP connection
/// on the given port so that debug commands and output are exchanged over the
/// network, keeping the BASIC program's stdin/stdout free from debugger traffic.
fn main() {
    let args: Vec<String> = env::args().collect();

    match parse_args(&args[1..]) {
        Ok(ParsedArgs::Version) => {
            println!("{}", version_string());
        }
        Ok(ParsedArgs::Run {
            filename,
            debug,
            debug_port,
        }) => {
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
                if let Some(port) = debug_port {
                    // Remote debug mode: listen for a TCP connection on the specified port.
                    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("Failed to bind to port {}: {}", port, e);
                            std::process::exit(1);
                        }
                    };
                    eprintln!("Debugger listening on 127.0.0.1:{}. Waiting for connection...", port);
                    let (stream, addr) = match listener.accept() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Failed to accept connection: {}", e);
                            std::process::exit(1);
                        }
                    };
                    eprintln!("Debugger connected from {}", addr);
                    let read_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Failed to clone TCP stream: {}", e);
                            std::process::exit(1);
                        }
                    };
                    let remote_input = Box::new(BufReader::new(read_stream));
                    let remote_output = Box::new(stream);
                    let interp = interpreter::Interpreter::new(stdin.lock(), stdout.lock());
                    let mut dbg = debugger::Debugger::new_remote(interp, remote_input, remote_output);
                    if let Err(e) = dbg.run_repl(&program) {
                        eprintln!("Debugger error: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    // Local debug mode: debugger uses stdin/stdout alongside the program.
                    let interp = interpreter::Interpreter::new(stdin.lock(), stdout.lock());
                    let mut dbg = debugger::Debugger::new(interp);
                    if let Err(e) = dbg.run_repl(&program) {
                        eprintln!("Debugger error: {}", e);
                        std::process::exit(1);
                    }
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
                debug_port: None,
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
                debug_port: None,
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
                debug_port: None,
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

    /// Tests that `--debug-port` is parsed correctly with `--debug`.
    #[test]
    fn test_parse_args_debug_port() {
        let args = vec![
            "--debug".to_string(),
            "--debug-port".to_string(),
            "9000".to_string(),
            "hello.bas".to_string(),
        ];
        assert_eq!(
            parse_args(&args),
            Ok(ParsedArgs::Run {
                filename: "hello.bas".to_string(),
                debug: true,
                debug_port: Some(9000),
            })
        );
    }

    /// Tests that `--debug-port` after the filename also works.
    #[test]
    fn test_parse_args_debug_port_after_filename() {
        let args = vec![
            "hello.bas".to_string(),
            "--debug".to_string(),
            "--debug-port".to_string(),
            "4567".to_string(),
        ];
        assert_eq!(
            parse_args(&args),
            Ok(ParsedArgs::Run {
                filename: "hello.bas".to_string(),
                debug: true,
                debug_port: Some(4567),
            })
        );
    }

    /// Tests that `--debug-port` without `--debug` is an error.
    #[test]
    fn test_parse_args_debug_port_without_debug() {
        let args = vec!["--debug-port".to_string(), "9000".to_string(), "hello.bas".to_string()];
        assert!(parse_args(&args).is_err());
    }

    /// Tests that `--debug-port` without a port number is an error.
    #[test]
    fn test_parse_args_debug_port_missing_value() {
        let args = vec!["--debug".to_string(), "--debug-port".to_string()];
        assert!(parse_args(&args).is_err());
    }

    /// Tests that `--debug-port` with an invalid port is an error.
    #[test]
    fn test_parse_args_debug_port_invalid() {
        let args = vec![
            "--debug".to_string(),
            "--debug-port".to_string(),
            "notanumber".to_string(),
            "hello.bas".to_string(),
        ];
        assert!(parse_args(&args).is_err());
    }

    /// Tests that `--debug-port` with a port exceeding u16 range is an error.
    #[test]
    fn test_parse_args_debug_port_out_of_range() {
        let args = vec![
            "--debug".to_string(),
            "--debug-port".to_string(),
            "99999".to_string(),
            "hello.bas".to_string(),
        ];
        assert!(parse_args(&args).is_err());
    }
}
