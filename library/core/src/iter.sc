/// Protocol for stateful producers of sequential values.
pub let iterator = trait {
  /// Element type yielded while the iterator is borrowed for `R`.
  let item(comptime r: region): type
  /// Advances the iterator and returns the next element, if any.
  let next(comptime r: region)(self: borrow(mut)(r)(self))
    (): core.option(item(r))
}

/// Protocol for values that can be converted into an iterator.
pub let into_iterator = trait {
  /// Iterator type produced from `Self`.
  let iter: type
  /// Consumes `self` and returns an iterator over its values.
  let into_iter(move self)
    (): iter
}

let array = core.memory.array
let slice = core.memory.slice

/// Constant item family for iterators that yield owned values.
pub let owned_item(comptime t: type)(comptime r: region): type = t

/// Access-preserving item family for iterators that yield element borrows.
pub let borrowed_item(comptime a: access, comptime t: type)(comptime r: region): type = borrow(a)(r)(t)

/// Owning iterator over a fixed-size array of copyable values.
pub let array_into_iter(comptime t: type)
  (comptime l: usize) = struct {
  values: array(t)(l),
  next_index: i32,
}

extend(comptime t: type, comptime l: usize) array_into_iter(t)(l): iterator
where t: core.marker.copyable {
  let item = owned_item(t)
  let next(comptime r: region)(self: borrow(mut)(r)(self))(): core.option(t) = {
    if self.next_index == l {
      core.option(t).none
    } else {
      let value = self.values[self.next_index]
      self.next_index = self.next_index + 1
      core.option(t).some(value)
    }
  }
}

extend(comptime t: type, comptime l: usize) array(t)(l): into_iterator
where t: core.marker.copyable {
  let iter = array_into_iter(t)(l)
  let into_iter(move self)(): array_into_iter(t)(l) = {
    array_into_iter(t)(l) { values: self, next_index: 0 }
  }
}

/// Access-preserving iterator over a borrowed slice.
pub let slice_iter(comptime a: access)(comptime t: type) = struct {
  values: borrow(a)(slice(t)),
  next_index: u64,
}

extend(comptime a: access, comptime t: type) slice_iter(a)(t): iterator {
  let item = borrowed_item(a, t)
  let next(comptime r: region)(self: borrow(mut)(r)(self))(): core.option(item(r)) = {
    if self.next_index == unsafe { raw_slice_len(self.values) } {
      none
    } else {
      let index = self.next_index
      self.next_index = self.next_index + 1
      some(unsafe {
        raw_slice_at(a)(self.values, index)
      })
    }
  }
}

extend(comptime a: access, comptime t: type) slice_iter(a)(t): into_iterator {
  let iter = slice_iter(a)(t)
  let into_iter(move self)(): slice_iter(a)(t) = { self }
}

extend(comptime t: type) slice(t) {
  /// Iterates over borrowed values while retaining source access.
  let iter(comptime a: access)(self: borrow(a)(self))(): slice_iter(a)(t) = {
    slice_iter(a)(t) { values: self, next_index: 0 }
  }
}
