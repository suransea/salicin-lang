// Private bridge from the compiler-generated test runner to the dedicated
// parent-owned result pipe. This is not general source I/O authority.
let host_report(
  index: u64,
  status: u8,
  has_message: u8,
  data: ptr(u8),
  length: u64,
): i32 = foreign(c, "sali_host_test_report")

let send(
  index: u64,
  status: u8,
  has_message: u8,
  data: ptr(u8),
  length: u64,
): () = {
  if unsafe { host_report(index, status, has_message, data, length) } != 0 {
    unsafe {
      raw_trap()
    }
  }
}

let report_pass(index: u64): bool = {
  let empty: u8 = 0
  send(index, 0, 0, ptr(borrow(empty)), 0)
  true
}

let report_with_message(
  index: u64,
  move message: core.string.string,
): bool = {
  let view = message.as_str()
  let bytes = view.as_bytes()
  let data = unsafe { raw_slice_ptr(bytes) }
  send(index, 1, 1, data, bytes.len())
  false
}

// Called only by the compiler-generated runner after one registration has
// returned through its source-backed failure handler and cleanup path.
let report(index: u64, move value: core.testing.outcome): bool = {
  match value
    { passed -> report_pass(index) }
    { failed(message) -> report_with_message(index, message) }
}

// Emits the terminal frame and returns the native process summary status.
let finish(registrations: u64, failures: u64): i32 = {
  let empty: u8 = 0
  send(registrations, 2, 0, ptr(borrow(empty)), failures)
  if failures == 0 { 0 } else { 1 }
}

/// Fails the current test with an exact owned UTF-8 message.
pub let fail: with(core.error.throwing(core.string.string))
  (move message: core.string.string): never = {
  core.error.throw(message)
}

/// Requires a condition to be true.
pub let assert: with(core.error.throwing(core.string.string))(condition: bool): () = {
  if !condition {
    fail("assertion failed")
  }
}

/// Converts values with the core diagnostic-formatting contract into owned
/// assertion text without exposing the assertion helpers' writer choice.
pub let assertion_debug = trait {
  let assertion_debug(self: borrow(self))(): core.string.string
}

extend(bool, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let value: bool = self
    if value { "true" } else { "false" }
  }
}

extend(core.string.unicode_scalar, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

extend(core.string.str, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    let mut scalars = self.scalars()
    while {
      match scalars.next()
        { some(scalar) ->
          writer.write_scalar(scalar)
          true
        }
        { none -> false }
    } {}
    writer.finish()
  }
}

extend(core.string.string, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

extend(u64, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

extend(u128, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

extend(i64, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

extend(i128, assertion_debug) {
  let assertion_debug(self: borrow(self))(): core.string.string = {
    let mut writer = alloc.string.string_writer.new()
    self.debug(writer)
    writer.finish()
  }
}

let equality_message(
  left: core.string.string,
  right: core.string.string,
): core.string.string = {
  let mut writer = alloc.string.string_writer.new()
  "assert_eq failed\nleft: ".display(writer)
  left.display(writer)
  "\nright: ".display(writer)
  right.display(writer)
  writer.finish()
}

let inequality_message(value: core.string.string): core.string.string = {
  let mut writer = alloc.string.string_writer.new()
  "assert_ne failed\nboth: ".display(writer)
  value.display(writer)
  writer.finish()
}

let unexpected_value_message(
  prefix: core.string.string,
)
  (value: core.string.string): core.string.string = {
  let mut writer = alloc.string.string_writer.new()
  prefix.display(writer)
  value.display(writer)
  ")".display(writer)
  writer.finish()
}

/// Requires two values to compare equal. Each operand is evaluated once.
pub let assert_eq(comptime t: type): with(core.error.throwing(core.string.string))
  (left: t)
  (right: t): () =
  requires(t is core.cmp.eq(t) && t is assertion_debug) {
  if !(left == right) {
    let left_text = left.assertion_debug()
    let right_text = right.assertion_debug()
    let message = equality_message(left_text, right_text)
    fail(message)
  }
}

/// Requires two values to compare unequal. Each operand is evaluated once.
pub let assert_ne(comptime t: type): with(core.error.throwing(core.string.string))
  (left: t)
  (right: t): () =
  requires(t is core.cmp.eq(t) && t is assertion_debug) {
  if left == right {
    let value_text = left.assertion_debug()
    let message = inequality_message(value_text)
    fail(message)
  }
}

/// Extracts `some`, failing when the option is empty.
pub let expect_some(comptime t: type): with(core.error.throwing(core.string.string))
  (move value: core.option(t)): t = {
  match value
    { some(value) -> value }
    { none -> fail("expect_some failed: found none") }
}

/// Requires `none`, formatting an unexpected payload exactly once.
pub let expect_none(comptime t: type): with(core.error.throwing(core.string.string))
  (move value: core.option(t)): () =
  requires(t is assertion_debug) {
  match value
    { none -> () }
    { some(value) ->
      let value_text = value.assertion_debug()
      let message = unexpected_value_message(
        "expect_none failed: found some(",
      )(value_text)
      fail(message)
    }
}

/// Extracts `ok`, formatting an unexpected error exactly once.
pub let expect_ok(comptime e: type, comptime t: type):
with(core.error.throwing(core.string.string))
  (move value: core.result(e)(t)): t =
  requires(e is assertion_debug) {
  match value
    { ok(value) -> value }
    { err(error) ->
      let error_text = error.assertion_debug()
      let message = unexpected_value_message(
        "expect_ok failed: found err(",
      )(error_text)
      fail(message)
    }
}

/// Extracts `err`, formatting an unexpected success value exactly once.
pub let expect_err(comptime e: type, comptime t: type):
with(core.error.throwing(core.string.string))
  (move value: core.result(e)(t)): e =
  requires(t is assertion_debug) {
  match value
    { err(error) -> error }
    { ok(value) ->
      let value_text = value.assertion_debug()
      let message = unexpected_value_message(
        "expect_err failed: found ok(",
      )(value_text)
      fail(message)
    }
}
