// Compile-time domains used by parameter passing, regions, and borrow(types).
/// Domain of compile-time type values.
pub let type = domain
/// Domain of compile-time lifetime regions.
pub let region = domain
/// Domain of compile-time effect rows and effect identities.
pub let effect = domain

/// Domain describing whether a borrow is shared or mutable.
pub let access = domain {
  /// Shared read-only access.
  shared
  /// Exclusive mutable access.
  mut
}

/// Domain describing how a runtime argument is passed to a callable.
pub let passing = domain {
  /// Lets the compiler choose copy, move, or borrow passing from context.
  auto
  /// Passes by copying the argument value.
  copy
  /// Passes by moving ownership of the argument value.
  move
}

/// Type constructor for a borrow with access `A`, region `'r`, and pointee `T`.
pub let borrow(A: access = shared)
  ('r: region)
  (T: type): type

/// Reborrows an existing borrowed value with the same access, region, and pointee.
pub let borrow(A: access = shared)
  ('r: region)
  (T: type)
  (value: borrow(A)('r)(T)): borrow(A)('r)(T)
