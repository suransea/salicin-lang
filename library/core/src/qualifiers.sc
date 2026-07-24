// Compile-time qualifiers and parameter-schema modifiers.
/// Describes whether a borrow is shared or mutable.
pub let access = type {
  /// Shared read-only access.
  shared,
  /// Exclusive mutable access.
  mut
}

/// Changes a runtime parameter schema to copy its argument.
pub let copy(P: parameters): parameters

/// Changes a runtime parameter schema to move its argument.
pub let move(P: parameters): parameters
