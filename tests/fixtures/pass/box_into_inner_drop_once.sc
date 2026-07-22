use alloc.boxed.{box_new, box_into_inner}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let boxed = box_new(T: Resource)(Resource { counter: counter })
    let resource = box_into_inner(boxed)
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  41 + drops
}
