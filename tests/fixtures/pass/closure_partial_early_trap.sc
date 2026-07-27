let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let escape(): i32 = {
  let base = 0
  let finish = { (move resource: resource)(value: i32) ->
    consume(resource)
    base + value
  }
  let pending = finish(resource { value: 0 })
  pending(return(42))
}

let main(): i32 = { escape() }

test("closure_partial_early_trap.sc") {
  main() == 42
}
