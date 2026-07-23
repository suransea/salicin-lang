let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

let make(counter: MutPtr(i32)): Array(Resource, 2) = { [Resource { counter: counter }, Resource { counter: counter }] }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let first = [Resource { counter: counter }, Resource { counter: counter }][0]
    let second = make(counter)[1]
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  38 + drops
}
