let box = alloc.boxed.box

let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
  }
}

let main(): i32 = {
  let boxed = box.new(resource { value: 1 })
  42
}

test("box_resource.sc") {
  std.test.assert(main() == 42)
}
