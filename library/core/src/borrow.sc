// Borrow access, type, and value contracts.
/// Describes whether a borrow is shared or mutable.
pub let access = sort {
  /// Shared read-only access.
  shared
  /// Exclusive mutable access.
  mut
}

/// Unqualified alias for `access.mut`.
pub let mut = access.mut
/// Unqualified alias for `access.shared`.
pub let shared = access.shared

/// Type constructor for a borrow with access `A`, region `R`, and pointee `T`.
pub let borrow(A: access = shared)
  (R: region)
  (T: type): type = builtin()

/// Creates or reborrows a borrow of an addressable pointee.
pub let borrow(A: access = shared)
  (R: region)
  (T: type)
  (value: T): borrow(A)(R)(T) = builtin()
