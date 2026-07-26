let option = std.option

let counter = struct { current: i32, end: i32 }

extend counter {
  let next(self: borrow(mut)(self))(): option(i32) = {
    if self.current < self.end {
      let value = self.current
      self.current = self.current + 1
      some(value)
    } else {
      none
    }
  }
}

let main(): i32 = {
  let mut counter = counter { current: 0, end: 7 }
  let mut total = 24
  loop {
    match counter.next()
      { some(value) ->
        if value < 3 {
          continue()
        }
        total = total + value
      }
      { none -> break() }
  }
  total
}

test("while_let.sc") {
  main() == 42
}
