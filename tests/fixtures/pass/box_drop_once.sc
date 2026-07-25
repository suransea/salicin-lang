let Box = std.boxed.Box

let Resource = struct { counter: Ptr(mut)(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  }
  }}

let main(): i32 = {
  let mut count = 0
  do {
    let counter = Ptr(mut)(borrow(mut)(count))
    let first = Box.new(T: Resource)(Resource { counter: counter })
    let second = first
  }
  41 + count
}

test("box_drop_once.sc") {
  main() == 42
}
