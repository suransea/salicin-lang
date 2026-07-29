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
let host_open(
  path: ptr(u8),
  flags: i32,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i32 = foreign(c, "sali_host_open")
let host_close(
  descriptor: i32,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i32 = foreign(c, "sali_host_close")
let host_flush(
  descriptor: i32,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i32 = foreign(c, "sali_host_flush")
let host_seek(
  descriptor: i32,
  offset: i64,
  origin: i32,
  failure: ptr(mut)(i32),
  raw_code: ptr(mut)(i32),
): i64 = foreign(c, "sali_host_seek")

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
  read_stream_at(0)(buffer)(0)
}

let read_stream_at: with(io)
  (descriptor: i32)
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
      descriptor,
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
    match read_stream_at(0)(buffer)(read)
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

/// Portable origins for file seeks.
pub let seek_from = enum { start, current, end }
extend(seek_from, core.marker.copyable) {}

/// Validated options for opening one native file.
pub let open_options = struct {
  read: bool,
  write: bool,
  append: bool,
  truncate: bool,
  create: bool,
  create_new: bool,
}
extend(open_options, core.marker.copyable) {}

extend(open_options) {
  let read_only(): open_options = {
    open_options { read: true, write: false, append: false, truncate: false, create: false, create_new: false }
  }
  let write_truncate(): open_options = {
    open_options { read: false, write: true, append: false, truncate: true, create: true, create_new: false }
  }
  let append_only(): open_options = {
    open_options { read: false, write: true, append: true, truncate: false, create: true, create_new: false }
  }
  let read_write(): open_options = {
    open_options { read: true, write: true, append: false, truncate: false, create: false, create_new: false }
  }
  let create_new(): open_options = {
    open_options { read: false, write: true, append: false, truncate: false, create: true, create_new: true }
  }
  let with_read(move self)(enabled: bool): open_options = {
    open_options { read: enabled, write: self.write, append: self.append, truncate: self.truncate, create: self.create, create_new: self.create_new }
  }
  let with_write(move self)(enabled: bool): open_options = {
    open_options { read: self.read, write: enabled, append: self.append, truncate: self.truncate, create: self.create, create_new: self.create_new }
  }
  let with_append(move self)(enabled: bool): open_options = {
    open_options { read: self.read, write: self.write, append: enabled, truncate: self.truncate, create: self.create, create_new: self.create_new }
  }
  let with_truncate(move self)(enabled: bool): open_options = {
    open_options { read: self.read, write: self.write, append: self.append, truncate: enabled, create: self.create, create_new: self.create_new }
  }
  let with_create(move self)(enabled: bool): open_options = {
    open_options { read: self.read, write: self.write, append: self.append, truncate: self.truncate, create: enabled, create_new: self.create_new }
  }
  let with_create_new(move self)(enabled: bool): open_options = {
    open_options { read: self.read, write: self.write, append: self.append, truncate: self.truncate, create: self.create, create_new: enabled }
  }
}

/// Unique owner of one native file descriptor.
pub let file = struct { descriptor: ptr(mut)(i32) }

let option_flags(options: open_options): core.result(io_error)(i32) = {
  if !options.read && !options.write {
    return(core.result.err(generated_error(invalid_input)))
  }
  if (options.append || options.truncate || options.create || options.create_new) &&
    !options.write {
    return(core.result.err(generated_error(invalid_input)))
  }
  if options.create_new && !options.create {
    return(core.result.err(generated_error(invalid_input)))
  }
  if options.append && options.truncate {
    return(core.result.err(generated_error(invalid_input)))
  }
  let mut flags: i32 = 0
  if options.write && !options.read { flags = flags + 1 }
  if options.read && options.write { flags = flags + 2 }
  if options.create { flags = flags + 4 }
  if options.create_new { flags = flags + 8 }
  if options.truncate { flags = flags + 16 }
  if options.append { flags = flags + 32 }
  core.result.ok(flags)
}

let descriptor(value: borrow(file)): i32 = {
  unsafe { *value.descriptor }
}

let close_descriptor: with(io)
  (value: ptr(mut)(i32)): core.result(io_error)(()) = {
  let descriptor = unsafe { *value }
  if descriptor < 0 { return(core.result.ok(())) }
  unsafe { *value = -1 }
  let mut failure: i32 = 12
  let mut raw_code: i32 = 0
  let status = unsafe {
    host_close(
      descriptor,
      ptr(mut)(borrow(mut)(failure)),
      ptr(mut)(borrow(mut)(raw_code)),
    )
  }
  if status == 0 {
    core.result.ok(())
  } else {
    core.result.err(host_error(failure, raw_code))
  }
}

/// Opens a UTF-8 path without normalization. Embedded NUL is rejected.
pub let open: with(io)
  (path: borrow(core.string.str))
  (options: open_options): core.result(io_error)(file) = {
  let flags = match option_flags(options)
    { ok(value) -> value }
    { err(error) -> return(core.result.err(error)) }
  let source = path.as_bytes()
  let length = source.len()
  let mut bytes = alloc.vec.vec(u8).with_capacity(length + 1)
  let mut index: u64 = 0
  while { index < length } {
    let byte = unsafe { *raw_offset(raw_slice_ptr(source), index) }
    if byte == 0 { return(core.result.err(generated_error(invalid_input))) }
    bytes.push(byte)
    index = index + 1
  }
  bytes.push(0)
  let view = bytes.as_slice()
  let mut failure: i32 = 12
  let mut raw_code: i32 = 0
  let descriptor = unsafe {
    host_open(
      raw_slice_ptr(view),
      flags,
      ptr(mut)(borrow(mut)(failure)),
      ptr(mut)(borrow(mut)(raw_code)),
    )
  }
  if descriptor < 0 {
    core.result.err(host_error(failure, raw_code))
  } else {
    let owner = alloc.boxed.box(i32).new(descriptor)
    core.result.ok(file { descriptor: owner.into_raw() })
  }
}

extend(file) {
  let read: with(io)
    (self: borrow(mut)(self))
    (buffer: borrow(mut)(core.memory.slice(u8))): core.result(io_error)(u64) = {
    read_stream_at(descriptor(self))(buffer)(0)
  }

  let read_exact: with(io)
    (self: borrow(mut)(self))
    (buffer: borrow(mut)(core.memory.slice(u8))): core.result(io_error)(()) = {
    let length = buffer.len(mut)()
    let mut read: u64 = 0
    while { read < length } {
      match read_stream_at(descriptor(self))(buffer)(read)
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

  let write: with(io)
    (self: borrow(self))
    (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(u64) = {
    write_stream_at(descriptor(self))(bytes)(0)
  }

  let write_all: with(io)
    (self: borrow(self))
    (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(()) = {
    write_all_stream(descriptor(self))(bytes)
  }

  let flush: with(io)
    (self: borrow(self))(): core.result(io_error)(()) = {
    let mut failure: i32 = 12
    let mut raw_code: i32 = 0
    let status = unsafe {
      host_flush(
        descriptor(self),
        ptr(mut)(borrow(mut)(failure)),
        ptr(mut)(borrow(mut)(raw_code)),
      )
    }
    if status == 0 {
      core.result.ok(())
    } else {
      core.result.err(host_error(failure, raw_code))
    }
  }

  let seek: with(io)
    (self: borrow(self))
    (offset: i64, origin: seek_from): core.result(io_error)(u64) = {
    let native_origin = match origin
      { start -> 0 }
      { current -> 1 }
      { end -> 2 }
    let mut failure: i32 = 12
    let mut raw_code: i32 = 0
    let position = unsafe {
      host_seek(
        descriptor(self),
        offset,
        native_origin,
        ptr(mut)(borrow(mut)(failure)),
        ptr(mut)(borrow(mut)(raw_code)),
      )
    }
    if position < 0 {
      core.result.err(host_error(failure, raw_code))
    } else {
      match position.checked_into(output: u64)()
        { some(value) -> core.result.ok(value) }
        { none -> core.result.err(generated_error(invalid_data)) }
    }
  }

  /// Consumes the owner. The descriptor is invalidated before the one close
  /// attempt, including when close reports an error.
  let close: with(io)(move self)(): core.result(io_error)(()) = {
    close_descriptor(self.descriptor)
  }
}

extend(file, core.marker.droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let descriptor = unsafe { *self.descriptor }
    if descriptor >= 0 {
      unsafe { *self.descriptor = -1 }
      let mut failure: i32 = 12
      let mut raw_code: i32 = 0
      let status = unsafe {
        host_close(
          descriptor,
          ptr(mut)(borrow(mut)(failure)),
          ptr(mut)(borrow(mut)(raw_code)),
        )
      }
    }
    let owner = unsafe { alloc.boxed.box(i32).from_raw(self.descriptor) }
  }
}

/// Reads a whole file while enforcing a caller-selected byte limit.
pub let read_file: with(io)
  (path: borrow(core.string.str))
  (limit: u64): core.result(io_error)(alloc.vec.vec(u8)) = {
  let mut input = match open(path)(open_options.read_only())
    { ok(value) -> value }
    { err(error) -> return(core.result.err(error)) }
  let mut output = alloc.vec.vec(u8).new()
  let mut done = false
  while { !done } {
    let mut chunk = alloc.vec.vec(u8).with_capacity(4096)
    let mut initialized: u64 = 0
    while { initialized < 4096 } {
      chunk.push(0)
      initialized = initialized + 1
    }
    let outcome = do {
      let buffer = chunk.as_slice(mut)()
      input.read(buffer)
    }
    match outcome
      { ok(0) -> done = true }
      { ok(count) ->
        if output.len() > limit || count > limit - output.len() {
          return(core.result.err(generated_error(invalid_data)))
        }
        let mut index: u64 = 0
        while { index < count } {
          output.push(chunk[index])
          index = index + 1
        }
      }
      { err(error) ->
        match error.kind()
          { interrupted -> () }
          { _ -> return(core.result.err(error)) }
      }
  }
  core.result.ok(output)
}

/// Creates or truncates a file and writes every byte.
pub let write_file: with(io)
  (path: borrow(core.string.str))
  (bytes: borrow(core.memory.slice(u8))): core.result(io_error)(()) = {
  let output = match open(path)(open_options.write_truncate())
    { ok(value) -> value }
    { err(error) -> return(core.result.err(error)) }
  match output.write_all(bytes)
    { err(error) -> core.result.err(error) }
    { ok(_) -> output.close() }
}
