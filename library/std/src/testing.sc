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

let report_without_message(index: u64): bool = {
  let empty: u8 = 0
  send(index, 1, 0, ptr(borrow(empty)), 0)
  false
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

let report_failure(
  index: u64,
  move message: core.option(core.string.string),
): bool = {
  match message
    { none -> report_without_message(index) }
    { some(message) -> report_with_message(index, message) }
}

// Called only by the compiler-generated runner after one registration has
// returned through its source-backed failure handler and cleanup path.
let report(index: u64, move value: core.testing.outcome): bool = {
  match value
    { passed -> report_pass(index) }
    { failed(message) -> report_failure(index, message) }
}

// Emits the terminal frame and returns the native process summary status.
let finish(registrations: u64, failures: u64): i32 = {
  let empty: u8 = 0
  send(registrations, 2, 0, ptr(borrow(empty)), failures)
  if failures == 0 { 0 } else { 1 }
}
