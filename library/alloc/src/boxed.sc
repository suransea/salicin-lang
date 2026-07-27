/// Owning heap allocation for a single value of type `T`.
pub let box(comptime t: type) = struct {
  /// Raw pointer to the initialized heap slot owned by this box.
  pointer: ptr(mut)(t),
}

/// Allocates heap storage and moves `value` into a new box.
let box_new(comptime t: type)(value: t): box(t) = {
  let pointer = unsafe {
    raw_alloc(t)(size_of(t), align_of(t))
  }
  unsafe {
    raw_init(pointer, value)
  }
  box(t) { pointer: pointer }
}

/// Consumes `boxed` without deallocating and returns its owned raw pointer.
let box_into_raw(comptime t: type)(move boxed: box(t)): ptr(mut)(t) = {
  let pointer = boxed.pointer
  forget(boxed)
  pointer
}

/// Copies the boxed value out of `boxed`.
let box_read(comptime t: type)(boxed: borrow(box(t))): t
where t: copyable = {
  unsafe {
    *boxed.pointer
  }
}

/// Copies `value` over the current boxed value.
let box_write(comptime t: type)(boxed: borrow(mut)(box(t)))(copy value: t): ()
where t: copyable = {
  unsafe {
    *boxed.pointer = value
  }
}

/// Consumes `boxed`, deallocates its storage, and returns the owned value.
let box_into_inner(comptime t: type)(move boxed: box(t)): t = {
  let pointer = boxed.pointer
  let value = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_dealloc(pointer, size_of(t), align_of(t))
  }
  forget(boxed)
  value
}

/// Replaces the boxed value and returns the previous value.
let box_replace(comptime t: type)(boxed: borrow(mut)(box(t)))(value: t): t = {
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
let box_as_ref(comptime a: access, comptime r: region, comptime t: type)
  (boxed: borrow(a)(r)(box(t))): borrow(a)(r)(t) = {
  unsafe {
    raw_borrow(a)(boxed.pointer, borrow(a)(boxed))
  }
}

/// Provides inherent constructors and accessors for `Box`.
extend(box(t)) {
  /// Allocates a new box containing `value`.
  let new(value: t): box(t) = { box_new(value) }
  /// Rebuilds unique ownership from a pointer returned by `Box.into_raw`.
  let from_raw(pointer: ptr(mut)(t)): box(t) with(core.unsafe.unsafe_effect) = {
    box(t) { pointer: pointer }
  }
  /// Borrows the boxed value with the requested access.
  let as_ref(comptime a: access)(self: borrow(a)(self))(): borrow(a)(t) = {
    unsafe {
      raw_borrow(a)(self.pointer, borrow(a)(self))
    }
  }
  /// Consumes this box and returns its owned value.
  let into_inner(move self)(): t = { box_into_inner(self) }
  /// Consumes this box without deallocating and returns its owned raw pointer.
  let into_raw(move self)(): ptr(mut)(t) = { box_into_raw(self) }
  /// Replaces the boxed value and returns the previous value.
  let replace(self: borrow(mut)(self))(value: t): t = { box_replace(self)(value) }
}

/// Provides copy-only value accessors for `Box`.
extend(box(t))
where t: copyable {
  /// Copies the boxed value out of this box.
  let read(self: borrow(self))(): t = { box_read(self) }
  /// Copies `value` over the current boxed value.
  let write(self: borrow(mut)(self))(copy value: t): () = { box_write(self)(value) }
}

/// Releases one box allocation after its value has been taken.
let box_deallocate(comptime t: type)(pointer: ptr(mut)(t)): () = {
  unsafe {
    raw_dealloc(pointer, size_of(t), align_of(t))
  }
}

/// Drops the owned value and releases its heap allocation.
extend(box(t), droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let pointer = self.pointer
    do {
      let value = unsafe {
        raw_take(pointer)
      }
    }
    box_deallocate(pointer)
  }
}
