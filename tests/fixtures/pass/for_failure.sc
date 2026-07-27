let option = std.option
let result = std.result
let throwing = std.error.throwing
let iterator = std.iter.iterator
let into_iterator = std.iter.into_iterator
let owned_item = std.iter.owned_item

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

let check(value: i32): () with(throwing(bool)) = {
  if value < 0 { throw(true) } else { () }
}

let visit(start: i32): i32 with(throwing(bool)) = {
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
