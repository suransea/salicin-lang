/// Protocol used by indexed place syntax.
pub let index(comptime key: type) = trait {
  /// Element type selected by the key.
  let output: type

  /// Borrows the selected element with the receiver's access.
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: key): borrow(a)(output)
}
