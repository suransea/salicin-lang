let Box = std.boxed.Box

let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }
}

let main(): i32 = {
  let boxed = Box.new(Resource { value: 0 })
  0
}

test("box_resource_drop_trap.sc") {
  main() == 42
}
