let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let finish(move resource: resource)(value: i32): i32 = { value }

let main(): i32 = {
  let pending = finish(resource { value: 0 })
  pending(return(0))
}

test("drop_partial_application_early_trap.sc") {
  main() == 42
}
