let resource = struct { value: i32 }
let holder = struct { resource: resource, tail: resource }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let replace_root(target: borrow(mut)(resource))(move replacement: resource): () = {
  target = replacement
}

let replace_field(target: borrow(mut)(holder))(move replacement: resource): () = {
  target.resource = replacement
}

let main(): i32 = {
  let mut resource = resource { value: 1 }
  replace_root(resource)(resource { value: 1 })
  let mut holder = holder { resource: resource { value: 1 }, tail: resource { value: 1 } }
  replace_field(holder)(resource { value: 1 })
  42
}

test("drop_mut_borrow_overwrite.sc") {
  std.test.assert(main() == 42)
}
