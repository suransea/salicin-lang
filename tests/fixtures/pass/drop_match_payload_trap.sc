let resource = struct { value: i32 }
let choice = enum { pair(resource, resource), none }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = { match choice.pair(resource { value: 1 }, resource { value: 0 })
    { pair(left, _) -> do {
        consume(left)
        0
      }
    }
    { none -> 0 }
}

test("drop_match_payload_trap.sc") {
  main() == 42
}
