let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let replace(target: borrow(mut)(resource))(move replacement: resource): () = {
  target = replacement
}

let main(): i32 = {
  let mut resource = resource { value: 0 }
  replace(resource)(resource { value: 1 })
  0
}

test("drop_mut_borrow_root_trap.sc") {
  main() == 42
}
