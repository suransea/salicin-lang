let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let new(move value: T): Cell(T) = { Cell { value: value } }
  let take(move self)(): T = { self.value }
  let replace(self: borrow(mut)(Self))(move value: T): () = {
    self.value = value
  }
}

let main(): i32 = {
  let inferred = Cell.new(40)
  let left = inferred.take()
  let mut explicit = Cell(i32).new(1)
  explicit.replace(2)
  left + explicit.take()
}
