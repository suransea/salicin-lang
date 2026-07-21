let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T) {
  let take(move self)(): T = { self.value }
  let round_trip(move value: T): T = {
    let cell = Cell(value)
    cell.take()
  }
}

let main(): i32 = { Cell.round_trip(42) }
