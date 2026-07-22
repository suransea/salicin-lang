let Read = trait {
  let read(borrow self)(): i32
}

let Cell(T: type) = struct { value: T }

extend Cell(i32): Read {
  let read(borrow self)(): i32 = { self.value }
}

extend(T: type) Cell(T) {
  let take(move self)(): T = { self.value }
}

let main(): i32 = {
  let cell = Cell(i32) { value: 42 }
  let read = Cell.read(cell)()
  let taken = Cell(i32).take(cell)()
  read + taken - 42
}
