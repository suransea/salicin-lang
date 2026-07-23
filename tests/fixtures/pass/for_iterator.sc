use std.Option

use std.iter.{Iterator, IntoIterator}

let Counter = struct { current: i32, end: i32 }

extend Counter: Iterator {
  let Item = i32

  let next(self: borrow(mut)(Self))(): Option(i32) = {
    if self.current < self.end {
      let value = self.current
      self.current = self.current + 1
      Some(value)
    } else {
      None
    }
  }}

extend Counter: IntoIterator {
  let IntoIter = Counter
  let into_iter(move self)(): Counter = { self }}

let main(): i32 = {
  let mut total = 21
  for value in Counter { current: 0, end: 7 } {
    total = total + value
  }
  total
}
