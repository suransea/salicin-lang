let option = core.option

let iterator = core.iter.iterator
let into_iterator = core.iter.into_iterator
let owned_item = core.iter.owned_item

let once = struct { done: bool }

extend(once, iterator) {
  let item = owned_item(i32)
  let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(i32) = {
    if self.done {
      none
    } else {
      self.done = true
      some(1)
    }
  }
}

extend(once, into_iterator) {
  let iter = once
  let into_iter(move self)(): once = { self }}

let main(): i32 = {
  for once { done: false } { value ->
    break(value)
  }
  0
}
