let resource = struct { value: i32 }
let choice = enum { pair(resource, resource), none }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let guard_false(move choice: choice): i32 = { match choice
    { pair(left, _) if left.value == 0 -> do {
        consume(left)
        0
      }
    }
    { pair(left, _) -> do {
        consume(left)
        21
      }
    }
    { none -> 0 }
}

let guard_true(move choice: choice): i32 = { match choice
    { pair(left, _) if left.value == 1 -> do {
        consume(left)
        21
      }
    }
    { pair(left, _) -> do {
        consume(left)
        0
      }
    }
    { none -> 0 }
}

let guard_return(move choice: choice): i32 = { match choice
    { pair(left, _) if return(42) -> 0 }
    { pair(left, _) -> do {
        consume(left)
        0
      }
    }
    { none -> 0 }
}

let main(): i32 = {
  let first = guard_false(choice.pair(resource { value: 1 }, resource { value: 1 }))
  let second = guard_true(choice.pair(resource { value: 1 }, resource { value: 1 }))
  let third = guard_return(choice.pair(resource { value: 1 }, resource { value: 1 }))
  first + second + third - 42
}

test("drop_match_guarded.sc") {
  main() == 42
}
