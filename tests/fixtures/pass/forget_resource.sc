let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trap = 1 / self.value
  }
}

let main(): i32 = {
  let resource = resource { value: 0 }
  forget(resource)
  42
}

test("forget_resource.sc") {
  main() == 42
}
