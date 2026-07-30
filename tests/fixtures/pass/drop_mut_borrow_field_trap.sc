let resource = struct { value: i32 }
let holder = struct { resource: resource, tail: resource }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let replace(target: borrow(mut)(holder))(move replacement: resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut holder = holder { resource: resource { value: 0 }, tail: resource { value: 1 } }
  replace(holder)(resource { value: 1 })
  0
}

test("drop_mut_borrow_field_trap.sc") {
  std.test.assert(main() == 42)
}
