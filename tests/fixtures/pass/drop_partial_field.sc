let resource = struct { value: i32 }
let pair = struct { left: resource, right: resource }
let nested = struct { pair: pair, tail: resource }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let conditional(flag: bool): () = {
  let pair = pair { left: resource { value: 1 }, right: resource { value: 1 } }
  if flag { consume(pair.left) }
}

let rebuild(): () = {
  let mut pair = pair { left: resource { value: 1 }, right: resource { value: 1 } }
  consume(pair.left)
  pair.left = resource { value: 1 }
}

let conditional_rebuild(flag: bool): () = {
  let mut pair = pair { left: resource { value: 1 }, right: resource { value: 1 } }
  if flag { consume(pair.left) }
  pair.left = resource { value: 1 }
}

let nested(): () = {
  let value = nested { pair: pair { left: resource { value: 1 }, right: resource { value: 1 } }, tail: resource { value: 1 } }
  consume(value.pair.left)
}

let main(): i32 = {
  conditional(true)
  conditional(false)
  rebuild()
  conditional_rebuild(true)
  conditional_rebuild(false)
  nested()
  42
}

test("drop_partial_field.sc") {
  main() == 42
}
