let option = core.option
let result = core.result
let throwing = core.error.throwing
let iterator = core.iter.iterator
let into_iterator = core.iter.into_iterator
let owned_item = core.iter.owned_item

let counter = struct { current: i32, end: i32 }

extend(counter, iterator) {
  let item = owned_item(i32)

  let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(i32) = {
    if self.current < self.end {
      let value = self.current
      self.current = self.current + 1
      some(value)
    } else {
      none
    }
  }
}

extend(counter, into_iterator) {
  let iter = counter

  let into_iter(move self)(): counter = {
    self
  }
}

let check: with(throwing(bool))(value: i32): () = {
  if value < 0 { throw(true) } else { () }
}

let visit: with(throwing(bool))(start: i32): i32 = {
  for counter { current: start, end: 4 } { value ->
    check(value)
  }
  42
}

let main(): i32 = {
  let success: result(bool)(i32) = try {
    visit(0)
  }
  let failure: result(bool)(i32) = try {
    visit(-1)
  }
  (success ?? 0) + (failure ?? 0)
}

test("for_failure.sc") {
  main() == 42
}
