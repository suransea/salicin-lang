let Box = std.boxed.Box

let Resource = struct { counter: Ptr(mut)(i32), value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
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
    let mut boxed = Box.new(T: Resource)(Resource { counter: counter, value: 10 })
    do {
      let previous = boxed.replace(Resource { counter: counter, value: 20 })
    }
  }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  40 + drops
}

test("box_replace_drop.sc") {
  main() == 42
}
