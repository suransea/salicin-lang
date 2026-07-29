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

extend(io_error) {
  let kind(self: borrow(self))(): io_error_kind = { self.failure }
  let raw_code(comptime r: region)
    (self: borrow(r)(self))(): borrow(r)(core.option(i32)) = {
    borrow(self.host_code)
  }
}
