let resource = struct { value: i32 }
let choice = enum { pair(resource, i32), none }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let choose(move choice: choice): i32 = { match choice
    { pair(resource, 42) -> do {
        consume(resource)
        21
      }
    }
    { pair(resource, _) -> do {
        consume(resource)
        21
      }
    }
    { none -> 0 }
}

let main(): i32 = {
  choose(choice.pair(resource { value: 1 }, 0)) +
    choose(choice.pair(resource { value: 1 }, 42))
}

test("match_literal_resource_guard.sc") {
  std.test.assert(main() == 42)
}
