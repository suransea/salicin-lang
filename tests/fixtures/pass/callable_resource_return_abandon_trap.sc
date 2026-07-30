let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let finish(move resource: resource)(value: i32): i32 = { value }

let make() = {
  let pending = finish(resource { value: 0 })
  pending
}

let main(): i32 = {
  let pending = make()
  42
}

test("callable_resource_return_abandon_trap.sc") {
  std.test.assert(main() == 42)
}
