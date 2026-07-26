let Option = std.Option

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
  let into_iter(move self)(): Counter = { self }}

let main(): i32 = {
  let mut total = 21
  for Counter { current: 0, end: 7 } { value ->
    total = total + value
  }
  total
}

test("for_iterator.sc") {
  main() == 42
}
