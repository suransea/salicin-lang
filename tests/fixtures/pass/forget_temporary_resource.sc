let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trap = 1 / self.value
  }
}

let main(): i32 = {
  forget(resource { value: 0 })
  42
}

test("forget_temporary_resource.sc") {
  main() == 42
}
