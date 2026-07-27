// Borrow access, type, and value contracts.
/// Describes whether a borrow is shared or mutable.
pub let access = sort(1) {
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
pub let borrow(comptime a: access = shared)
  (comptime r: region)
  (comptime t: type): type = builtin()

/// Creates or reborrows a borrow of an addressable pointee.
pub let borrow(comptime a: access = shared)
  (comptime r: region)
  (comptime t: type)
  (value: t): borrow(a)(r)(t) = builtin()
