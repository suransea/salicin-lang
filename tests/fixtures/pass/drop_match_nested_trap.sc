let resource = struct { value: i32 }
let bundle = struct { left: resource, right: resource }
let choice = enum { some(bundle, resource), none }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = { match choice.some(
    bundle { left: resource { value: 1 }, right: resource { value: 0 } },
    resource { value: 1 }
  )
    { some(bundle(left: left, right: _), _) -> do {
        consume(left)
        0
      }
    }
    { none -> 0 }
}

test("drop_match_nested_trap.sc") {
  main() == 42
}
