let Read = trait {
  let read(borrow self)(): i32
}

let Leaf = struct { value: i32 }

extend Leaf: Read {
  let read(borrow self)(): i32 = { self.value }
}

let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T): Read
where T: Read {
  let read(borrow self)(): i32 = { self.value.read() }
}

let read_cell(T: type)(borrow cell: Cell(T)): i32
where T: Read = { cell.read() }

let Value = trait {
  let Item: type
  let take(move self)(): Item
}

extend(T: type) Cell(T): Value {
  let Item = T
  let take(move self)(): T = { self.value }
}

let main(): i32 = {
  let cell = Cell { value: Leaf { value: 42 } }
  let read = read_cell(cell)
  let leaf = cell.take()
  let wrapped = Cell { value: leaf }
  wrapped.read() + read - 42
}
