let resource = struct { value: i32 }

extend resource {
  let increment(self: borrow(mut)(self))(): i32 = {
    self.value = self.value + 1
    self.value
  }
}

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let main(): i32 = { resource { value: 41 }.increment() }

test("inherent_temporary_mut_resource_receiver.sc") {
  main() == 42
}
