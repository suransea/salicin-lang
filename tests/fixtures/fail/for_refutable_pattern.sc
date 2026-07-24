let Option = std.Option
let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator

let Values = struct { done: bool }
let Choice = enum { Some(i32), None }

extend Values: Iterator {
  let Item = Choice

  let next(self: borrow(mut)(Self))(): Option(Choice) = {
    if self.done {
      None
    } else {
      self.done = true
      Some(Choice.Some(42))
    }
  }
}

extend Values: IntoIterator {
  let IntoIter = Values
  let into_iter(move self)(): Values = { self }
}

let main(): i32 = {
  for Values { done: false } { Choice.Some(value) ->
    value
  }
  0
}
