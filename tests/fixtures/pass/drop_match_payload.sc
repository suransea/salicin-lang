let resource = struct { value: i32 }
let choice = enum { pair(resource, resource), none }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let inspect(move choice: choice): i32 = { match choice
    { pair(left, _) -> do {
        consume(left)
        42
      }
    }
    { none -> 0 }
}

let escape(move choice: choice): i32 = { match choice
    { pair(left, _) -> do {
        consume(left)
        return(42)
      }
    }
    { none -> 0 }
}

let main(): i32 = {
  let first = inspect(choice.pair(resource { value: 1 }, resource { value: 1 }))
  let second = escape(choice.pair(resource { value: 1 }, resource { value: 1 }))
  first + second - 42
}

test("drop_match_payload.sc") {
  main() == 42
}
