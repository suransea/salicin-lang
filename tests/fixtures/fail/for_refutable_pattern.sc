let option = std.option
let iterator = std.iter.iterator
let into_iterator = std.iter.into_iterator
let owned_item = std.iter.owned_item

let values = struct { done: bool }
let choice = enum { some(i32), none }

extend values: iterator {
  let item = owned_item(choice)

  let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(choice) = {
    if self.done {
      none
    } else {
      self.done = true
      some(choice.some(42))
    }
  }
}

extend values: into_iterator {
  let into_iter = values
  let into_iter(move self)(): values = { self }
}

let main(): i32 = {
  for values { done: false } { choice.some(value) ->
    value
  }
  0
}
