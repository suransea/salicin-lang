use super::support::*;
use std::io::Write;
use std::process::Stdio;

#[test]
fn file_authority_and_ownership_are_static_contracts() {
    let pure = check_source(
        r#"let misuse(path: borrow(core.string.str)): core.result(std.io.io_error)(std.io.file) = {
  std.io.open(path)(std.io.open_options.read_only())
}
let main(): i32 = { 42 }"#,
    )
    .expect_err("pure file open must require io");
    assert!(
        pure.iter()
            .any(|diagnostic| diagnostic.contains("requires custom effect `std::io::io`")),
        "{pure:#?}"
    );

    let copied = check_source(
        r#"let duplicate(copy value: std.io.file): () = {}
let main(): i32 = { 42 }"#,
    )
    .expect_err("file owners must not be copyable");
    assert!(
        copied
            .iter()
            .any(|diagnostic| diagnostic.contains("does not implement copyable")),
        "{copied:#?}"
    );
}

#[test]
fn explicit_close_failure_invalidates_before_the_single_host_attempt() {
    let source = r#"let close_calls(): i32 = foreign(c, "close_calls")

let abandon: with(std.io.io)
  (path: borrow(core.string.str)): () = {
  match std.io.open(path)(std.io.open_options.read_only())
    { ok(value) -> () }
    { err(_) -> () }
}

let main: with(std.io.io)(): i32 = {
  let path: string = "/dev/null"
  let view = path.as_str()
  let input = match std.io.open(view)(std.io.open_options.read_only())
    { ok(value) -> value }
    { err(_) -> return(1) }
  match input.close()
    { ok(_) -> return(2) }
    { err(_) -> () }
  if unsafe { close_calls() } != 1 { return(3) }
  abandon(view)
  if unsafe { close_calls() } == 2 { 42 } else { 5 }
}"#;
    let ir = compile_source(source).expect("compile close-failure fixture");
    let output = link_and_run_ir_with_c(
        &ir,
        r#"#include <errno.h>
static int calls;
int close(int descriptor) {
  (void)descriptor;
  calls += 1;
  errno = EIO;
  return -1;
}
int close_calls(void) {
  return calls;
}"#,
        "close failure invalidation",
    );
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

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

#[test]
fn native_file_owners_support_options_seek_flush_limits_and_close() {
    let temporary = TestDirectory::new();
    let source = temporary.write(
        "files.sc",
        r#"let main: with(std.io.io)(): i32 = {
  let mut arguments = match std.io.arguments()
    { ok(value) -> value }
    { err(_) -> return(1) }
  let path = arguments.remove(1)
  let missing = arguments.remove(1)
  let path_view = path.as_str()
  let missing_view = missing.as_str()
  let initial: array(u8)(3) = "abc"
  let initial_view = initial.as_slice()
  match std.io.write_file(path_view)(initial_view)
    { err(_) -> return(2) }
    { ok(_) -> () }
  match std.io.read_file(path_view)(2)
    { err(error) ->
      match error.kind()
        { invalid_data -> () }
        { _ -> return(3) }
    }
    { ok(_) -> return(4) }
  let mut output = match std.io.open(path_view)(std.io.open_options.append_only())
    { ok(value) -> value }
    { err(_) -> return(5) }
  let suffix: array(u8)(1) = "d"
  let suffix_view = suffix.as_slice()
  match output.write_all(suffix_view)
    { err(_) -> return(6) }
    { ok(_) -> () }
  match output.flush()
    { err(_) -> return(7) }
    { ok(_) -> () }
  match output.close()
    { err(_) -> return(8) }
    { ok(_) -> () }
  match std.io.open(path_view)(std.io.open_options.create_new())
    { err(error) ->
      match error.kind()
        { already_exists -> () }
        { _ -> return(9) }
    }
    { ok(_) -> return(10) }
  match std.io.open(missing_view)(std.io.open_options.read_only())
    { err(error) ->
      match error.kind()
        { not_found -> () }
        { _ -> return(11) }
    }
    { ok(_) -> return(12) }
  let invalid_options = std.io.open_options.read_only().with_create(true)
  match std.io.open(path_view)(invalid_options)
    { err(error) ->
      match error.kind()
        { invalid_input -> () }
        { _ -> return(18) }
    }
    { ok(_) -> return(19) }
  let mut nul_bytes = alloc.vec.vec(u8).new()
  nul_bytes.push(97)
  nul_bytes.push(0)
  nul_bytes.push(98)
  let nul_path = match alloc.string.string_from_utf8(nul_bytes)
    { ok(value) -> value }
    { err(_) -> return(20) }
  let nul_view = nul_path.as_str()
  match std.io.open(nul_view)(std.io.open_options.read_only())
    { err(error) ->
      match error.kind()
        { invalid_input -> () }
        { _ -> return(21) }
    }
    { ok(_) -> return(22) }
  let mut input = match std.io.open(path_view)(std.io.open_options.read_only())
    { ok(value) -> value }
    { err(_) -> return(13) }
  match input.seek(0, end)
    { ok(4) -> () }
    { _ -> return(14) }
  match input.seek(0, start)
    { ok(0) -> () }
    { _ -> return(15) }
  let mut bytes: array(u8)(4) = [0, 0, 0, 0]
  let outcome = do {
    let buffer = bytes.as_slice(mut)()
    input.read_exact(buffer)
  }
  match outcome
    { err(_) -> return(16) }
    { ok(_) -> () }
  if bytes[0] == 97 && bytes[1] == 98 && bytes[2] == 99 && bytes[3] == 100 {
    42
  } else {
    17
  }
}"#,
    );
    let executable = temporary.join("files");
    let built = salic()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build native file fixture");
    assert!(built.status.success(), "{}", output_text(&built));

    let path = temporary.join("data.bin");
    let missing = temporary.join("missing.bin");
    let output = Command::new(&executable)
        .arg(&path)
        .arg(&missing)
        .output()
        .expect("run native file fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
    assert_eq!(fs::read(path).expect("read file fixture"), b"abcd");
}
