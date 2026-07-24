// Borrow access, type, and value contracts.
/// Describes whether a borrow is shared or mutable.
pub let access = enum {
  /// Shared read-only access.
  shared,
  /// Exclusive mutable access.
  mut
}

/// Type constructor for a borrow with access `A`, region `R`, and pointee `T`.
pub let borrow(A: access = shared)
  (R: region)
  (T: type): type

/// Creates or reborrows a borrow of an addressable pointee.
pub let borrow(A: access = shared)
  (R: region)
  (T: type)
  (value: T): borrow(A)(R)(T)
