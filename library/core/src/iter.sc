/// Protocol for stateful producers of sequential values.
pub let Iterator = trait {
  /// Element type yielded while the iterator is borrowed for `R`.
  let Item(R: region): type
  /// Advances the iterator and returns the next element, if any.
  let next(R: region)(self: borrow(mut)(R)(Self))
    (): core.Option(Item(R))
}

/// Protocol for values that can be converted into an iterator.
pub let IntoIterator = trait {
  /// Iterator type produced from `Self`.
  let IntoIter: type
  /// Consumes `self` and returns an iterator over its values.
  let into_iter(move self)
    (): IntoIter
}

let Array = core.memory.Array
let Slice = core.memory.Slice

/// Constant item family for iterators that yield owned values.
pub let OwnedItem(T: type)(R: region): type = T

/// Access-preserving item family for iterators that yield element borrows.
pub let BorrowedItem(A: access, T: type)(R: region): type = borrow(A)(R)(T)

/// Owning iterator over a fixed-size array of copyable values.
pub let ArrayIntoIter(T: type)
  (L: usize) = struct {
  values: Array(T)(L),
  next_index: i32,
}

extend(T: type, L: usize) ArrayIntoIter(T)(L): Iterator
where T: core.marker.Copy {
  let Item = OwnedItem(T)
  let next(R: region)(self: borrow(mut)(R)(Self))(): core.Option(T) = {
    if self.next_index == L {
      core.Option(T).None
    } else {
      let value = self.values[self.next_index]
      self.next_index = self.next_index + 1
      core.Option(T).Some(value)
    }
  }
}

extend(T: type, L: usize) Array(T)(L): IntoIterator
where T: core.marker.Copy {
  let IntoIter = ArrayIntoIter(T)(L)
  let into_iter(move self)(): ArrayIntoIter(T)(L) = {
    ArrayIntoIter(T)(L) { values: self, next_index: 0 }
  }
}

/// Access-preserving iterator over a borrowed slice.
pub let SliceIter(A: access)(T: type) = struct {
  values: borrow(A)(Slice(T)),
  next_index: u64,
}

extend(A: access, T: type) SliceIter(A)(T): Iterator {
  let Item = BorrowedItem(A, T)
  let next(R: region)(self: borrow(mut)(R)(Self))(): core.Option(Item(R)) = {
    if self.next_index == unsafe { raw_slice_len(self.values) } {
      None
    } else {
      let index = self.next_index
      self.next_index = self.next_index + 1
      Some(unsafe {
        raw_slice_at(A)(self.values, index)
      })
    }
  }
}

extend(A: access, T: type) SliceIter(A)(T): IntoIterator {
  let IntoIter = SliceIter(A)(T)
  let into_iter(move self)(): SliceIter(A)(T) = { self }
}

extend(T: type) Slice(T) {
  /// Iterates over borrowed values while retaining source access.
  let iter(A: access)(self: borrow(A)(Self))(): SliceIter(A)(T) = {
    SliceIter(A)(T) { values: self, next_index: 0 }
  }
}
