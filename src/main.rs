//! Entry point for the Qwen BASIC interpreter.
//!
//! This module reads a BASIC source file, tokenizes it, parses it into an AST,
//! and executes it through the interpreter. The pipeline is:
//! source text -> Lexer (tokens) -> Parser (AST) -> Interpreter (execution).

mod ast;
mod eval;
mod expr;
mod interpreter;
mod token;

use std::env;
use std::fs;
use std::io;

/// Runs the BASIC interpreter: reads the source file specified as a command-line
/// argument, tokenizes and parses it, then executes the resulting program.
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: qwen_basic <filename.bas>");
        std::process::exit(1);
    }

    let filename = &args[1];
    let source = match fs::read_to_string(filename) {
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
    let mut interp = interpreter::Interpreter::new(stdin.lock(), stdout.lock());

    if let Err(e) = interp.run(&program) {
        eprintln!("Runtime error: {}", e);
        std::process::exit(1);
    }
}
