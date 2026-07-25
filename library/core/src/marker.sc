/// Auto marker for types whose owning value may be safely relocated.
pub let Move = trait {}

/// Marker trait for types that may be duplicated by implicit copy.
pub let Copy = trait
where Self: Move {}

/// Trait for types that need cleanup when their owning value leaves scope.
pub let Drop = trait {
  /// Releases resources owned by `self`.
  let drop(self: borrow(mut)(Self))
    (): ()
}
