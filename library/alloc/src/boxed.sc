pub let Box(T: type) = struct { pointer: MutPtr(T) }

pub let box_new(T: type)(move value: T): Box(T) = {
  let pointer = unsafe {
    raw_alloc(T)(size_of(T), align_of(T))
  }
  unsafe {
    raw_init(pointer, value)
  }
  Box(T) { pointer: pointer }
}

pub let box_ptr(T: type)(boxed: borrow(Box(T))): MutPtr(T) = { boxed.pointer }

pub let box_read(T: type)(boxed: borrow(Box(T))): T
where T: Copy = {
  unsafe {
    *boxed.pointer
  }
}

pub let box_write(T: type)(boxed: borrow(mut)(Box(T)))(copy value: T): ()
where T: Copy = {
  unsafe {
    *boxed.pointer = value
  }
}

pub let box_into_inner(T: type)(move boxed: Box(T)): T = {
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

pub let box_replace(T: type)(boxed: borrow(mut)(Box(T)))(move value: T): T = {
  let pointer = boxed.pointer
  let previous = unsafe {
    raw_take(pointer)
  }
  unsafe {
    raw_init(pointer, value)
  }
  previous
}

pub let box_as_ref(A: access, 'a: region, T: type)
  (boxed: borrow(A)('a)(Box(T))): borrow(A)('a)(T) = {
  unsafe {
    raw_borrow(A)(boxed.pointer, borrow(A)(boxed))
  }
}

extend(T: type) Box(T) {
  let new(move value: T): Box(T) = { box_new(value) }
  let as_mut_ptr(self: borrow(Self))(): MutPtr(T) = { box_ptr(self) }
  let as_ref(A: access)(self: borrow(A)(Self))(): borrow(A)(T) = {
    unsafe {
      raw_borrow(A)(self.pointer, borrow(A)(self))
    }
  }
  let into_inner(move self)(): T = { box_into_inner(self) }
  let replace(self: borrow(mut)(Self))(move value: T): T = { box_replace(self)(value) }
}

extend(T: type) Box(T)
where T: Copy {
  let read(self: borrow(Self))(): T = { box_read(self) }
  let write(self: borrow(mut)(Self))(copy value: T): () = { box_write(self)(value) }
}
