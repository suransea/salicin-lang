/// Protocol used by indexed place syntax.
pub let Index(Key: type) = trait {
  /// Element type selected by the key.
  let Output: type

  /// Borrows the selected element with the receiver's access.
  let index(A: access)
    (self: borrow(A)(Self))
    (key: Key): borrow(A)(Output)
}
