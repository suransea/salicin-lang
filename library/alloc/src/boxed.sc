/// Owning heap allocation for a single value of type `T`.
pub let Box(T: type) = struct {
  /// Raw pointer to the initialized heap slot owned by this box.
  pointer: Ptr(mut)(T),
}

/// Allocates heap storage and moves `value` into a new box.
let box_new(T: type)(value: T): Box(T) = {
  let pointer = unsafe {
    raw_alloc(T)(size_of(T), align_of(T))
  }
  unsafe {
    raw_init(pointer, value)
  }
  Box(T) { pointer: pointer }
}

/// Consumes `boxed` without deallocating and returns its owned raw pointer.
let box_into_raw(T: type)(move boxed: Box(T)): Ptr(mut)(T) = {
  let pointer = boxed.pointer
  forget(boxed)
  pointer
}

/// Copies the boxed value out of `boxed`.
let box_read(T: type)(boxed: borrow(Box(T))): T
where T: Copy = {
  unsafe {
    *boxed.pointer
  }
}

/// Copies `value` over the current boxed value.
let box_write(T: type)(boxed: borrow(mut)(Box(T)))(copy value: T): ()
where T: Copy = {
  unsafe {
    *boxed.pointer = value
  }
}

/// Consumes `boxed`, deallocates its storage, and returns the owned value.
let box_into_inner(T: type)(move boxed: Box(T)): T = {
  let pointer = boxed.pointer
  let value = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_dealloc(pointer, size_of(T), align_of(T))
  }
  forget(boxed)
  value
}

/// Replaces the boxed value and returns the previous value.
let box_replace(T: type)(boxed: borrow(mut)(Box(T)))(value: T): T = {
  let pointer = boxed.pointer
  let previous = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_init(pointer, value)
  }
  previous
}

/// Borrows the boxed value with the same access and region as `boxed`.
let box_as_ref(A: access, R: region, T: type)
  (boxed: borrow(A)(R)(Box(T))): borrow(A)(R)(T) = {
  unsafe {
    raw_borrow(A)(boxed.pointer, borrow(A)(boxed))
  }
}

/// Provides inherent constructors and accessors for `Box`.
extend(T: type) Box(T) {
  /// Allocates a new box containing `value`.
  let new(value: T): Box(T) = { box_new(value) }
  /// Rebuilds unique ownership from a pointer returned by `Box.into_raw`.
  let from_raw(pointer: Ptr(mut)(T)): Box(T) with(core.effect.Unsafe) = {
    Box(T) { pointer: pointer }
  }
  /// Borrows the boxed value with the requested access.
  let as_ref(A: access)(self: borrow(A)(Self))(): borrow(A)(T) = {
    unsafe {
      raw_borrow(A)(self.pointer, borrow(A)(self))
    }
  }
  /// Consumes this box and returns its owned value.
  let into_inner(move self)(): T = { box_into_inner(self) }
  /// Consumes this box without deallocating and returns its owned raw pointer.
  let into_raw(move self)(): Ptr(mut)(T) = { box_into_raw(self) }
  /// Replaces the boxed value and returns the previous value.
  let replace(self: borrow(mut)(Self))(value: T): T = { box_replace(self)(value) }
}

/// Provides copy-only value accessors for `Box`.
extend(T: type) Box(T)
where T: Copy {
  /// Copies the boxed value out of this box.
  let read(self: borrow(Self))(): T = { box_read(self) }
  /// Copies `value` over the current boxed value.
  let write(self: borrow(mut)(Self))(copy value: T): () = { box_write(self)(value) }
}
