/// Protocol for stateful producers of sequential values.
pub let Iterator = trait {
  /// Element type yielded by this iterator.
  let Item: type
  /// Advances the iterator and returns the next element, if any.
  let next(self: borrow(mut)(Self))
    (): core.Option(Item)
}

/// Protocol for values that can be converted into an iterator.
pub let IntoIterator = trait {
  /// Iterator type produced from `Self`.
  let IntoIter: type
  /// Consumes `self` and returns an iterator over its values.
  let into_iter(move self)
    (): IntoIter
}
