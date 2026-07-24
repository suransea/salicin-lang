let box_new = std.boxed.box_new

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

let main(): i32 = {
  let mut count = 0
  do {
    let counter = MutPtr(borrow(mut)(count))
    let first = box_new(T: Resource)(Resource { counter: counter })
    let second = first
  }
  41 + count
}
