let Resource = struct { counter: MutPtr(i32) }
let Batch = struct { values: Array(Resource, 2) }

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
    let batch = Batch { values: [Resource { counter: counter }, Resource { counter: counter }] }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  40 + drops
}
