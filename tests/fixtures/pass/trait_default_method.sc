let Read = trait {
  let read(borrow self)(): i32
  let doubled(borrow self)(): i32 = { self.read() + self.read() }
}

let Number = struct { value: i32 }

extend Number: Read {
  let read(borrow self)(): i32 = { self.value }
}

let Override = struct {}

extend Override: Read {
  let read(borrow self)(): i32 = { 0 }
  let doubled(borrow self)(): i32 = { 42 }
}

let Cell (T: type) = struct { value: T }

extend(T: type) Cell(T): Read
where T: Read {
  let read(borrow self)(): i32 = { self.value.read() }
}

let Take = trait {
  let Item: type
  let take(move self)(): Item
  let forward(move self)(): Item = { self.take() }
}

let Boxed = struct { value: i32 }

extend Boxed: Take {
  let Item = i32
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number { value: 21 }
  let cell = Cell { value: number }
  let overridden = Override {}
  let boxed = Boxed { value: 42 }
  cell.doubled() + overridden.doubled() + boxed.forward() - 84
}
