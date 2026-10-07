//! Integration test that loads a real `.bas` file from disk and runs it through
//! the actual compiled CLI, the same way a user would. This exercises the full
//! `main()` path (file read -> lexer -> parser -> interpreter) end-to-end, which
//! the in-module unit tests (which feed inline source strings) do not.
//!
//! The `.bas` file ends with a STOP statement: in non-debug mode the
//! interpreter halts there, prints "Break in line 60" to stderr, and exits
//! cleanly, having preserved all program state from the statements that ran
//! before it. A second test drives the same pipeline in `--debug` mode by
//! piping a debugger command script into stdin, and a third test exercises the
//! `--debug --debug-port` remote mode by connecting a real TCP client to the
//! listener the binary spawns.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

const BASIC_FILE: &str = "examples/integration-test.bas";
const DEBUG_BASIC_FILE: &str = "examples/integration-debug.bas";
const DEBUG_COMMANDS_FILE: &str = "examples/integration-debug.cmds";
const REMOTE_DEBUG_BASIC_FILE: &str = "examples/integration-remote-debug.bas";

/// Path to the CLI binary cargo built alongside the test binaries
/// (current_exe() lives in `target/<profile>/deps/`; the binary is one level up).
fn binary_path() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("vibe-basic")
}

/// Run the integration `.bas` file through the real CLI in non-debug mode and
/// assert the STOP halts execution at line 60, reporting the break on stderr.
#[test]
fn integration_test_bas_file() {
    let output = Command::new(binary_path())
        .arg(BASIC_FILE)
        .output()
        .expect("failed to launch the vibe-basic binary");

    assert!(
        output.status.success(),
        "expected a clean exit, got {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // The STOP is the last statement (line 60); non-debug execution halts there
    // and reports the break on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Break in line 60"),
        "expected 'Break in line 60' on stderr, got:\n{stderr}"
    );
}

/// Writes a string to a uniquely-named temp `.bas` file (unique per test via the
/// `name` tag, since integration tests run in parallel within one process) and
/// returns its path.
fn write_temp_bas(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("vibe-basic-integration-{}-{}.bas", std::process::id(), name));
    std::fs::write(&path, contents).expect("failed to write temp .bas file");
    path
}

/// Run the CLI against a nonexistent file: it must exit non-zero and report a
/// file-read error on stderr (exercising main()'s error-exit harness).
#[test]
fn integration_missing_file_exits_nonzero() {
    let missing = "no-such-file-for-integration-test-9f2b7a.bas";
    let output = Command::new(binary_path())
        .arg(missing)
        .output()
        .expect("failed to launch the vibe-basic binary");

    assert!(
        !output.status.success(),
        "expected a non-zero exit for a missing file, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error reading file"),
        "expected a file-read error on stderr, got:\n{stderr}"
    );
}

/// Run the CLI against a file with malformed BASIC: it must exit non-zero and
/// report a parse error on stderr.
#[test]
fn integration_parse_error_exits_nonzero() {
    let path = write_temp_bas("parse", "10 IF\n");
    let output = Command::new(binary_path())
        .arg(&path)
        .output()
        .expect("failed to launch the vibe-basic binary");
    let _ = std::fs::remove_file(&path);

    assert!(
        !output.status.success(),
        "expected a non-zero exit for a parse error, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Parse error"),
        "expected a parse error on stderr, got:\n{stderr}"
    );
}

/// Run the CLI against a program that raises a runtime error (division by zero):
/// it must exit non-zero and report a runtime error on stderr.
#[test]
fn integration_runtime_error_exits_nonzero() {
    let path = write_temp_bas("runtime", "10 LET X = 1/0\n");
    let output = Command::new(binary_path())
        .arg(&path)
        .output()
        .expect("failed to launch the vibe-basic binary");
    let _ = std::fs::remove_file(&path);

    assert!(
        !output.status.success(),
        "expected a non-zero exit for a runtime error, got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Runtime error"),
        "expected a runtime error on stderr, got:\n{stderr}"
    );
}

/// Run the CLI with `--version`: it must exit cleanly and print the version string
/// (exercising the `ParsedArgs::Version` arm of `run`).
#[test]
fn integration_version_flag_prints_version() {
    let output = Command::new(binary_path())
        .arg("--version")
        .output()
        .expect("failed to launch the vibe-basic binary");

    assert!(
        output.status.success(),
        "expected a clean exit for --version, got {:?}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("vibe-basic "),
        "expected a 'vibe-basic <version>' string, got:\n{stdout}"
    );
}

/// Run the debug-mode integration program through the real CLI in `--debug` mode,
/// piping a script of debugger commands from disk into stdin. This exercises the
/// `main()` debug branch and the full interactive debugger REPL (RUN/STOP break,
/// PRINT/LET inspection, GOTO jump, and a program INPUT fed from the command
/// stream) end-to-end, which the non-debug integration test cannot reach.
#[test]
fn integration_test_debug_bas_file() {
    let commands = std::fs::read_to_string(DEBUG_COMMANDS_FILE)
        .unwrap_or_else(|e| panic!("failed to read {DEBUG_COMMANDS_FILE}: {e}"));

    let mut child = Command::new(binary_path())
        .arg("--debug")
        .arg(DEBUG_BASIC_FILE)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to launch the vibe-basic binary");

    // Feed the debugger command script, then close stdin (EOF) so the REPL
    // terminates deterministically even if the last command is not QUIT.
    {
        let stdin = child.stdin.as_mut().expect("stdin not piped");
        stdin
            .write_all(commands.as_bytes())
            .expect("failed to write debug commands");
    }
    let output = child.wait_with_output().expect("failed to wait on debugger process");

    assert!(
        output.status.success(),
        "expected a clean exit, got {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // In local debug mode all debugger output (prompts, messages) goes to stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The STOP at line 40 halts execution and is reported at the break point.
    assert!(
        stdout.contains("Break in line 40"),
        "expected 'Break in line 40' in output:\n{stdout}"
    );
    // The debugger's PRINT X inspected the preserved state (X = 40).
    assert!(
        stdout.contains("40"),
        "expected inspected variable X=40 in output:\n{stdout}"
    );
    // After resuming past the STOP, the program ran to completion.
    assert!(
        stdout.contains("Program finished."),
        "expected 'Program finished.' in output:\n{stdout}"
    );
}

/// Run the remote-debug integration program through the real CLI in
/// `--debug --debug-port <port>` mode. The binary binds a TCP listener and
/// exchanges the debugger REPL over the connection, while the BASIC program's
/// own `PRINT` still goes to the child's stdout. This exercises the remote
/// `main()` branch (port binding, accepting a connection, cloning the stream,
/// and running the REPL with remote I/O) end-to-end.
#[test]
fn integration_test_remote_debug_bas_file() {
    // Reserve an ephemeral port and immediately drop the listener so the
    // spawned binary can bind it; a brief retry handles the rare race where
    // the kernel has not fully released the address yet.
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind a test port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut child = Command::new(binary_path())
        .arg("--debug")
        .arg("--debug-port")
        .arg(port.to_string())
        .arg(REMOTE_DEBUG_BASIC_FILE)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to launch the vibe-basic binary");

    // Connect to the listener the binary spawned, retrying briefly while the
    // child starts up.
    let mut stream = None;
    for _ in 0..100 {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
    let mut stream = match stream {
        Some(s) => s,
        None => {
            child.kill().ok();
            let output = child.wait_with_output().expect("failed to wait on process");
            panic!(
                "could not connect to remote debugger on port {port}\nstderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("failed to set read timeout");

    // Drive the remote debugger: RUN to completion, then QUIT.
    stream
        .write_all(b"RUN\nQUIT\n")
        .expect("failed to send remote debug commands");

    // Read the debugger output until the child closes the connection.
    let mut response = String::new();
    let _ = std::io::Read::read_to_string(&mut stream, &mut response);
    drop(stream);

    let output = child
        .wait_with_output()
        .expect("failed to wait on remote debug process");

    assert!(
        output.status.success(),
        "expected a clean exit, got {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // The program's own PRINT goes to the child's stdout (not the TCP stream).
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("VALUE: 7"),
        "expected program output 'VALUE: 7' on stdout, got:\n{stdout}"
    );

    // The debugger status messages go to the child's stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Debugger listening"),
        "expected a 'Debugger listening' message on stderr, got:\n{stderr}"
    );

    // The debugger REPL flows over the TCP connection, not the child's stdout.
    assert!(
        response.contains("Program finished."),
        "expected 'Program finished.' over TCP, got:\n{response}"
    );
    assert!(
        response.contains("[DBG"),
        "expected debugger prompt over TCP, got:\n{response}"
    );
    // Confirm the program output did NOT leak onto the debug stream.
    assert!(
        !response.contains("VALUE:"),
        "program output leaked onto the debug TCP stream:\n{response}"
    );
}
