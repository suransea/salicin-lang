let option = core.option

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
  let into_iter(move self)(): counter = { self }}

let main(): i32 = {
  let mut total = 21
  for counter { current: 0, end: 7 } { value ->
    total = total + value
  }
  total
}

test("for_iterator.sc") {
  std.test.assert(main() == 42)
}
