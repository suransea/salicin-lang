/// The uninhabited type used for computations that never return normally.
pub let Never = enum {}

/// Marker trait for types that may be duplicated by implicit copy.
pub let Copy = trait {}

/// Trait for types that need cleanup when their owning value leaves scope.
pub let Drop = trait {
  /// Releases resources owned by `self`.
  let drop(self: borrow(mut)(Self))
    (): ()
}
