let Option = std.Option

let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator

let Once = struct { done: bool }

extend Once: Iterator {
  let Item = i32
  let next(self: borrow(mut)(Self))(): Option(i32) = {
    if self.done {
      None
    } else {
      self.done = true
      Some(1)
    }
  }}

extend Once: IntoIterator {
  let IntoIter = Once
  let into_iter(move self)(): Once = { self }}

let main(): i32 = {
  for Once { done: false } { value ->
    break(value)
  }
  0
}
