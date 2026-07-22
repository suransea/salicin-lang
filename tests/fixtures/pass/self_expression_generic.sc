let Rewrap = trait {
  let rewrap(move self)(): Self
}

let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let wrap(move value: T): Self = { Self { value: value } }
  let replace(move self)(move value: T): Self = { Self { value: value } }
}

extend(T: type) Cell(T): Rewrap {
  let rewrap(move self)(): Self = { Self { value: self.value } }
}

let main(): i32 = {
  let first = Cell.wrap(20).value
  let cell = Cell.wrap(0)
  let second = cell.replace(22).rewrap().value
  first + second
}
