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

let Array = core.memory.Array
let Slice = core.memory.Slice

let iter_copy(T: type)(value: borrow(T)): T
where T: core.marker.Copy = { value }

/// Owning iterator over a fixed-size array of copyable values.
pub let ArrayIntoIter(T: type)
  (L: usize) = struct {
  values: Array(T)(L),
  next_index: i32,
}

extend(T: type, L: usize) ArrayIntoIter(T)(L): Iterator
where T: core.marker.Copy {
  let Item = T
  let next(self: borrow(mut)(Self))(): core.Option(T) = {
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

/// Shared iterator over a borrowed slice of copyable values.
pub let SliceIter(T: type) = struct {
  values: borrow(Slice(T)),
  next_index: u64,
}

extend(T: type) SliceIter(T)
where T: core.marker.Copy {
  /// Creates an iterator retaining the supplied slice loan.
  let new(values: borrow(Slice(T))): SliceIter(T) = {
    SliceIter(T) { values: values, next_index: 0 }
  }
}

extend(T: type) SliceIter(T): Iterator
where T: core.marker.Copy {
  let Item = T
  let next(self: borrow(mut)(Self))(): core.Option(T) = {
    if self.next_index == unsafe { raw_slice_len(self.values) } {
      core.Option(T).None
    } else {
      let value = unsafe {
        raw_slice_at(shared)(self.values, self.next_index)
      }
      self.next_index = self.next_index + 1
      core.Option(T).Some(iter_copy(value))
    }
  }
}

extend(T: type) SliceIter(T): IntoIterator
where T: core.marker.Copy {
  let IntoIter = SliceIter(T)
  let into_iter(move self)(): SliceIter(T) = { self }
}

extend(T: type) Slice(T)
where T: core.marker.Copy {
  /// Iterates over copied values while retaining the source slice loan.
  let iter(self: borrow(Self))(): SliceIter(T) = {
    SliceIter(T).new(self)
  }
}
