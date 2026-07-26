let resource = struct { value: i32 }

extend resource {
  let read(self: borrow(self))(): i32 = { self.value }
}

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let main(): i32 = { resource { value: 42 }.read() }

test("inherent_temporary_resource_receiver.sc") {
  main() == 42
}
