let box = std.boxed.box

let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let main(): i32 = {
  let boxed = box.new(resource { value: 0 })
  0
}

test("box_resource_drop_trap.sc") {
  main() == 42
}
