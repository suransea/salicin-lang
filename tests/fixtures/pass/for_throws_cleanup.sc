let option = std.option
let result = std.result
let throws = std.error.throws
let iterator = std.iter.iterator
let into_iterator = std.iter.into_iterator
let owned_item = std.iter.owned_item

let counter = struct {
  current: i32,
  end: i32,
  drops: ptr(mut)(i32),
}

extend counter: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

extend counter: iterator {
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

extend counter: into_iterator {
  let iter = counter

  let into_iter(move self)(): counter = {
    self
  }
}

let check(value: i32): () with(throws(bool)) = {
  if value < 0 { throw(true) } else { () }
}

let visit(move counter: counter): i32 with(throws(bool)) = {
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

  let success: result(bool)(i32) = try {
    visit(counter { current: 0, end: 2, drops: drops })
  }
  let failure: result(bool)(i32) = try {
    visit(counter { current: -1, end: 2, drops: drops })
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
