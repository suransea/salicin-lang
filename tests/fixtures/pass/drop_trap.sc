let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let main(): i32 = {
  let value = resource { value: 0 }
  0
}

test("drop_trap.sc") {
  main() == 42
}
