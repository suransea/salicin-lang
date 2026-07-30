let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }
let consume_pair(move left: resource, move right: resource): () = { () }

let invoke(): i32 = {
  let resource = resource { value: 1 }
  let once = { consume(resource) }
  once()
  42
}

let abandon(): () = {
  let resource = resource { value: 1 }
  let once = { consume(resource) }
}

let invoke_pair(): () = {
  let left = resource { value: 1 }
  let right = resource { value: 1 }
  let once = { consume_pair(left, right) }
  once()
}

let conditional(flag: bool): () = {
  let resource = resource { value: 1 }
  let once = { consume(resource) }
  if flag { once() }
}

let early(): i32 = {
  let resource = resource { value: 1 }
  let once = {
    (value: i32) -> do {
      consume(resource)
      value
    }
  }
  once(return(42))
}

let main(): i32 = {
  let answer = invoke()
  abandon()
  invoke_pair()
  conditional(true)
  conditional(false)
  answer + early() - 42
}

test("drop_closure_once.sc") {
  std.test.assert(main() == 42)
}
