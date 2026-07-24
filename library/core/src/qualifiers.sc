// Closed compile-time qualifier types used by borrows and parameter schemas.
/// Describes whether a borrow is shared or mutable.
pub let access = type {
  /// Shared read-only access.
  shared,
  /// Exclusive mutable access.
  mut
}

/// Describes how a runtime argument is passed to a callable.
pub let passing = type {
  /// Lets the compiler choose copy, move, or borrow passing from context.
  auto,
  /// Passes by copying the argument value.
  copy,
  /// Passes by moving ownership of the argument value.
  move
}
