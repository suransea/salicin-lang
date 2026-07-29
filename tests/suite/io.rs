use super::support::*;
use std::io::Write;
use std::process::Stdio;

#[test]
fn native_console_and_process_contracts_preserve_bytes_and_utf8() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "io.sc",
        r#"let main: with(std.io.io)(): i32 = {
  let argument = match std.io.argument_bytes(1)
    { some(value) -> value }
    { none -> return(1) }
  if argument.len() != 3 || argument[0] != 255 || argument[1] != 111 || argument[2] != 107 {
    return(2)
  }
  match std.io.arguments()
    { ok(_) -> return(3) }
    { err(error) ->
      match error.kind()
        { invalid_data -> () }
        { _ -> return(4) }
    }
  let maybe_line = match std.io.read_line()
    { ok(value) -> value }
    { _ -> return(5) }
  let line = match maybe_line
    { some(value) -> value }
    { none -> return(5) }
  let expected: string = "hello\n"
  if line != expected { return(6) }
  let output: string = "stdout"
  let error: string = "stderr"
  let output_view = output.as_str()
  match std.io.println(output_view)
    { err(_) -> return(7) }
    { ok(_) -> () }
  let error_view = error.as_str()
  match std.io.eprintln(error_view)
    { err(_) -> return(8) }
    { ok(_) -> 42 }
}"#,
    );
    let executable = temporary.join("io-program");
    let built = salic()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build native I/O fixture");
    assert!(built.status.success(), "{}", output_text(&built));

    use std::os::unix::ffi::OsStringExt;
    let mut child = Command::new(&executable)
        .arg(std::ffi::OsString::from_vec(vec![255, b'o', b'k']))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn native I/O fixture");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(b"hello\n")
        .expect("write fixture stdin");
    let output = child.wait_with_output().expect("wait for I/O fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
    assert_eq!(output.stdout, b"stdout\n");
    assert_eq!(output.stderr, b"stderr\n");
}

#[test]
fn native_io_helpers_report_eof_and_broken_pipe() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "io-errors.sc",
        r#"let main: with(std.io.io)(): i32 = {
  let mode = match std.io.argument_bytes(1)
    { some(value) -> value }
    { none -> return(1) }
  if mode[0] == 101 {
    let mut bytes: array(u8)(2) = [0, 0]
    let outcome = do {
      let buffer = bytes.as_slice(mut)()
      std.io.read_stdin_exact(buffer)
    }
    match outcome
      { err(error) ->
        match error.kind()
          { unexpected_eof -> 42 }
          { _ -> 2 }
      }
      { ok(_) -> 3 }
  } else {
    let mut bytes = alloc.vec.vec(u8).with_capacity(1048576)
    let mut index: u64 = 0
    while { index < 1048576 } {
      bytes.push(120)
      index = index + 1
    }
    let view = bytes.as_slice()
    match std.io.write_stdout_all(view)
      { err(error) ->
        match error.kind()
          { broken_pipe -> 42 }
          { _ -> 4 }
      }
      { ok(_) -> 5 }
  }
}"#,
    );
    let executable = temporary.join("io-errors");
    let built = salic()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build native I/O error fixture");
    assert!(built.status.success(), "{}", output_text(&built));

    let eof = Command::new(&executable)
        .arg("eof")
        .stdin(Stdio::piped())
        .output()
        .expect("run EOF fixture");
    assert_eq!(eof.status.code(), Some(42), "{}", output_text(&eof));

    let mut broken = Command::new(&executable)
        .arg("pipe")
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn broken-pipe fixture");
    drop(broken.stdout.take());
    let status = broken.wait().expect("wait for broken-pipe fixture");
    assert_eq!(status.code(), Some(42));
}
