/// Visible authority to interact with the native host environment.
///
/// The native entry boundary is the only implicit handler for this exact
/// standard identity. Importing this module grants no authority by itself.
pub let io = effect {}

/// Portable classifications for recoverable synchronous host failures.
pub let io_error_kind = enum {
  not_found,
  permission_denied,
  already_exists,
  invalid_input,
  invalid_data,
  interrupted,
  would_block,
  write_zero,
  unexpected_eof,
  broken_pipe,
  unsupported,
  out_of_memory,
  other,
}

extend(io_error_kind, core.marker.copyable) {}

/// A portable failure classification plus an optional signed host code.
///
/// Portable control flow must inspect `kind`; `raw_code` is diagnostic data
/// and is never interpreted as a stable cross-platform value.
pub let io_error = struct {
  failure: io_error_kind,
  host_code: core.option(i32),
}

/// Lossless storage for one process argument.
pub let process_argument = struct { bytes: alloc.vec.vec(u8) }

extend(process_argument) {
  let into_bytes(move self)(): alloc.vec.vec(u8) = { self.bytes }
}

extend(io_error) {
  let kind(self: borrow(self))(): io_error_kind = { self.failure }
  let raw_code(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(core.option(i32)) = {
    borrow(self.host_code)
  }
}

// These bridges are private, fixed-shape runtime contracts. Their C
// implementations are emitted by the native backend and their names are
// reserved from user foreign declarations.
let host_read(
  descriptor: i32,
  data: ptr(mut)(u8),
  length: u64,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i64 = foreign(c, "sali_host_read")

let host_write(
  descriptor: i32,
  data: ptr(u8),
  length: u64,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i64 = foreign(c, "sali_host_write")

let host_argument_count(): u64 = foreign(c, "sali_host_argument_count")
let host_argument_length(index: u64): u64 = foreign(c, "sali_host_argument_length")
let host_argument_byte(index: u64, offset: u64): u8 =
  foreign(c, "sali_host_argument_byte")

let decode_error_kind(value: i32): io_error_kind = {
  if value == 0 { not_found }
  else if value == 1 { permission_denied }
  else if value == 2 { already_exists }
  else if value == 3 { invalid_input }
  else if value == 4 { invalid_data }
  else if value == 5 { interrupted }
  else if value == 6 { would_block }
  else if value == 7 { write_zero }
  else if value == 8 { unexpected_eof }
  else if value == 9 { broken_pipe }
  else if value == 10 { unsupported }
  else if value == 11 { out_of_memory }
  else { other }
}

let host_error(failure: i32, raw_code: i32): io_error = {
  io_error { failure: decode_error_kind(failure), host_code: core.option.some(raw_code) }
}

let generated_error(failure: io_error_kind): io_error = {
  io_error { failure: failure, host_code: core.option.none }
}

let count_result(
  count: i64,
  failure: i32,
  raw_code: i32,
): core.result(io_error)(u64) = {
  if count < 0 {
    core.result.err(host_error(failure, raw_code))
  } else {
    match count.checked_into(output: u64)()
      { some(value) -> core.result.ok(value) }
      { none -> core.result.err(generated_error(invalid_data)) }
  }
}

/// Performs one synchronous read attempt from standard input.
///
/// A successful zero count on a non-empty buffer is EOF. Short reads are
/// successful and `interrupted` is preserved.
pub let read_stdin: with(io)
  (buffer: borrow(mut)(core.memory.slice(u8))): core.result(io_error)(u64) = {
  read_stream_at(buffer)(0)
}

let read_stream_at: with(io)
  (buffer: borrow(mut)(core.memory.slice(u8)))
  (offset: u64): core.result(io_error)(u64) = {
  let length = buffer.len(mut)()
  if offset >= length {
    return(core.result.ok(0))
  }
  let mut failure: i32 = 12
  let mut raw_code: i32 = 0
  let count = unsafe {
    let data = raw_offset(raw_slice_ptr(mut)(buffer), offset)
    host_read(
      0,
      data,
      length - offset,
      ptr(mut)(borrow(mut)(failure)),
      ptr(mut)(borrow(mut)(raw_code)),
    )
  }
  count_result(count, failure, raw_code)
}

/// Performs one synchronous write attempt to standard output.
pub let write_stdout: with(io)
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(u64) = {
  write_stream_at(1)(bytes)(0)
}

/// Performs one synchronous write attempt to standard error.
pub let write_stderr: with(io)
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(u64) = {
  write_stream_at(2)(bytes)(0)
}

let write_stream_at: with(io)
  (descriptor: i32)
  (bytes: borrow(core.memory.slice(u8)))
  (offset: u64): core.result(io_error)(u64) = {
  let length = bytes.len()
  if offset >= length {
    return(core.result.ok(0))
  }
  let mut failure: i32 = 12
  let mut raw_code: i32 = 0
  let count = unsafe {
    let data = raw_offset(raw_slice_ptr(bytes), offset)
    host_write(
      descriptor,
      data,
      length - offset,
      ptr(mut)(borrow(mut)(failure)),
      ptr(mut)(borrow(mut)(raw_code)),
    )
  }
  count_result(count, failure, raw_code)
}

let write_all_stream: with(io)
  (descriptor: i32)
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(()) = {
  let length = bytes.len()
  let mut written: u64 = 0
  while { written < length } {
    match write_stream_at(descriptor)(bytes)(written)
      { ok(0) -> return(core.result.err(generated_error(write_zero))) }
      { ok(count) -> written = written + count }
      { err(error) ->
        match error.kind()
          { interrupted -> () }
          { _ -> return(core.result.err(error)) }
      }
  }
  core.result.ok(())
}

/// Writes every byte to standard output, retrying interruption.
pub let write_stdout_all: with(io)
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(()) = {
  write_all_stream(1)(bytes)
}

/// Writes every byte to standard error, retrying interruption.
pub let write_stderr_all: with(io)
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(()) = {
  write_all_stream(2)(bytes)
}

/// Direct descriptor writes are unbuffered, so flushing is a successful
/// synchronization point without another host call.
pub let flush_stdout: with(io)(): core.result(io_error)(()) = { core.result.ok(()) }
pub let flush_stderr: with(io)(): core.result(io_error)(()) = { core.result.ok(()) }

/// Writes validated UTF-8 text to standard output.
pub let print: with(io)
  (value: borrow(core.string.str)): core.result(io_error)(()) = {
  let bytes = value.as_bytes()
  write_stdout_all(bytes)
}

/// Writes validated UTF-8 text followed by one LF byte.
pub let println: with(io)
  (value: borrow(core.string.str)): core.result(io_error)(()) = {
  match print(value)
    { err(error) -> core.result.err(error) }
    { ok(_) ->
      let newline: array(u8)(1) = [10]
      let bytes = newline.as_slice()
      write_stdout_all(bytes)
    }
}

/// Writes validated UTF-8 text to standard error.
pub let eprint: with(io)
  (value: borrow(core.string.str)): core.result(io_error)(()) = {
  let bytes = value.as_bytes()
  write_stderr_all(bytes)
}

/// Writes validated UTF-8 text followed by one LF byte to standard error.
pub let eprintln: with(io)
  (value: borrow(core.string.str)): core.result(io_error)(()) = {
  match eprint(value)
    { err(error) -> core.result.err(error) }
    { ok(_) ->
      let newline: array(u8)(1) = [10]
      let bytes = newline.as_slice()
      write_stderr_all(bytes)
    }
}

/// Reads exactly `buffer.len()` bytes or reports `unexpected_eof`.
pub let read_stdin_exact: with(io)
  (buffer: borrow(mut)(core.memory.slice(u8))): core.result(io_error)(()) = {
  let length = buffer.len(mut)()
  let mut read: u64 = 0
  while { read < length } {
    match read_stream_at(buffer)(read)
      { ok(0) -> return(core.result.err(generated_error(unexpected_eof))) }
      { ok(count) -> read = read + count }
      { err(error) ->
        match error.kind()
          { interrupted -> () }
          { _ -> return(core.result.err(error)) }
      }
  }
  core.result.ok(())
}

/// Reads one UTF-8 line including its LF terminator. EOF before any byte is
/// `none`; invalid UTF-8 is `invalid_data`.
pub let read_line: with(io)(): core.result(io_error)(core.option(string)) = {
  let mut bytes = alloc.vec.vec(u8).new()
  let mut done = false
  while { !done } {
    let mut byte: array(u8)(1) = [0]
    let outcome = do {
      let buffer = byte.as_slice(mut)()
      read_stdin(buffer)
    }
    match outcome
      { ok(0) -> done = true }
      { ok(_) ->
        let value = byte[0]
        bytes.push(value)
        if value == 10 { done = true }
      }
      { err(error) ->
        match error.kind()
          { interrupted -> () }
          { _ -> return(core.result.err(error)) }
      }
  }
  if bytes.is_empty() {
    core.result.ok(core.option.none)
  } else {
    match alloc.string.string_from_utf8(bytes)
      { ok(text) -> core.result.ok(core.option.some(text)) }
      { err(_) -> core.result.err(generated_error(invalid_data)) }
  }
}

/// Returns the number of process arguments, including the executable name.
pub let argument_count: with(io)(): u64 = {
  unsafe { host_argument_count() }
}

/// Copies one process argument losslessly as Unix bytes.
pub let argument_bytes: with(io)
  (index: u64): core.option(alloc.vec.vec(u8)) = {
  let count = argument_count()
  if index >= count {
    core.option.none
  } else {
    let length = unsafe { host_argument_length(index) }
    let mut bytes = alloc.vec.vec(u8).with_capacity(length)
    let mut offset: u64 = 0
    while { offset < length } {
      bytes.push(unsafe { host_argument_byte(index, offset) })
      offset = offset + 1
    }
    core.option.some(bytes)
  }
}

/// Copies all process arguments losslessly as Unix bytes.
pub let arguments_bytes: with(io)(): alloc.vec.vec(process_argument) = {
  let count = argument_count()
  let mut arguments = alloc.vec.vec(process_argument).with_capacity(count)
  let mut index: u64 = 0
  while { index < count } {
    match argument_bytes(index)
      { some(argument) -> arguments.push(process_argument { bytes: argument }) }
      { none -> () }
    index = index + 1
  }
  arguments
}

/// Copies all process arguments as validated UTF-8 text.
///
/// The first invalid host argument returns `invalid_data`; no replacement
/// decoding is performed.
pub let arguments: with(io)(): core.result(io_error)(alloc.vec.vec(string)) = {
  let mut bytes = arguments_bytes()
  let count = bytes.len()
  let mut text = alloc.vec.vec(string).with_capacity(count)
  let mut index: u64 = 0
  while { index < count } {
    let argument = bytes.remove(0).into_bytes()
    match alloc.string.string_from_utf8(argument)
      { ok(value) -> text.push(value) }
      { err(_) -> return(core.result.err(generated_error(invalid_data))) }
    index = index + 1
  }
  core.result.ok(text)
}
