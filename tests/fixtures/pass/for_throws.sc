let Option = std.Option
let Result = std.Result
let Throws = std.effect.Throws
let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator
let OwnedItem = std.iter.OwnedItem

let Counter = struct { current: i32, end: i32 }

extend Counter: Iterator {
  let Item = OwnedItem(i32)

  let next(R: region)(self: borrow(mut)(R)(Self))(): Option(i32) = {
    if self.current < self.end {
      let value = self.current
      self.current = self.current + 1
      Some(value)
    } else {
      None
    }
  }
}

extend Counter: IntoIterator {
  let IntoIter = Counter

  let into_iter(move self)(): Counter = {
    self
  }
}

let check(value: i32): () with(Throws(bool)) = {
  if value < 0 { throw(true) } else { () }
}

let visit(start: i32): i32 with(Throws(bool)) = {
  for Counter { current: start, end: 4 } { value ->
    check(value)
  }
  42
}

let main(): i32 = {
  let success: Result(bool)(i32) = try {
    visit(0)
  }
  let failure: Result(bool)(i32) = try {
    visit(-1)
  }
  (success ?? 0) + (failure ?? 0)
}
