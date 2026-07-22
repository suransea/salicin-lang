let Cell(T: type) = struct { value: T }
let Holder = struct { cell: Cell(i32) }

extend(T: type) Cell(T) {
  let take(move self)(): T = { self.value }
}

let main(): i32 = {
  let holder = Holder { cell: Cell { value: 42 } }
  holder.cell.take()
}
