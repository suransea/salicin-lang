let resource = struct { value: i32 }
let bundle = struct { left: resource, right: resource }
let choice = enum { some(bundle, resource), none }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let inspect(move choice: choice): i32 = { match choice
    { some(bundle(left: left, right: _), _) -> do {
        consume(left)
        return(42)
      }
    }
    { none -> 0 }
}

let main(): i32 = { inspect(
    choice.some(bundle { left: resource { value: 1 }, right: resource { value: 1 } }, resource { value: 1 })
  )
}

test("drop_match_nested.sc") {
  main() == 42
}
