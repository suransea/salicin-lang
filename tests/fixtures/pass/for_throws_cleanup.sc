let Option = std.Option
let Result = std.Result
let Throws = std.error.Throws
let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator
let OwnedItem = std.iter.OwnedItem

let Counter = struct {
  current: i32,
  end: i32,
  drops: Ptr(mut)(i32),
}

extend Counter: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

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

let visit(move counter: Counter): i32 with(Throws(bool)) = {
  for counter { value ->
    check(value)
  }
  42
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let success: Result(bool)(i32) = try {
    visit(Counter { current: 0, end: 2, drops: drops })
  }
  let failure: Result(bool)(i32) = try {
    visit(Counter { current: -1, end: 2, drops: drops })
  }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  (success ?? 0) + (failure ?? 0) + drop_count - 2
}

test("for_throws_cleanup.sc") {
  main() == 42
}
