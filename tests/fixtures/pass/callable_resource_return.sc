let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move resource: resource): () = { () }

let finish(move resource: resource)(value: i32): i32 = {
  consume(resource)
  value
}

let make() = {
  let pending = finish(resource { value: 1 })
  pending
}

let main(): i32 = {
  let original = make()
  let pending = original
  pending(42)
}

test("callable_resource_return.sc") {
  main() == 42
}
