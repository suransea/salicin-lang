/// Auto marker for types whose owning value may be safely relocated.
pub let movable = trait {}

/// Marker trait for types that may be duplicated by implicit copy.
pub let copyable = trait
where self: movable {}

/// Trait for types that need cleanup when their owning value leaves scope.
pub let droppable = trait {
  /// Releases resources owned by `self`.
  let drop(self: borrow(mut)(self))
    (): ()
}
