let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let take(move self)(): T = { self.value }
}

let consume(T: type)(move cell: Cell(T)): T = { cell.take() }

let main(): i32 = { consume(cell: Cell { value: 42 }) }
