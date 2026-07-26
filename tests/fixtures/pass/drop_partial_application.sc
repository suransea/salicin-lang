let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let finish(move resource: resource)(value: i32): i32 = {
  consume(resource)
  value
}

let finish_curried(move resource: resource)(left: i32)(right: i32): i32 = {
  consume(resource)
  left + right
}

let invoke(): i32 = {
  let pending = finish(resource { value: 1 })
  pending(42)
}

let continue_partial(): i32 = {
  let first = finish_curried(resource { value: 1 })
  let second = first(20)
  second(22)
}

let abandon(): () = {
  let pending = finish(resource { value: 1 })
}

let conditional(flag: bool): () = {
  let pending = finish(resource { value: 1 })
  if flag { pending(0); }
}

let early(): i32 = {
  let pending = finish(resource { value: 1 })
  pending(return(42))
}

let main(): i32 = {
  let first = invoke()
  let second = continue_partial()
  abandon()
  conditional(true)
  conditional(false)
  let third = early()
  first + second + third - 84
}

test("drop_partial_application.sc") {
  main() == 42
}
