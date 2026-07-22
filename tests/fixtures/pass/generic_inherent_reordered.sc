let Pair (A: type, B: type) = struct { first: A, second: B }

extend(X: type, Y: type) Pair(Y, X) {
  let new(move first: Y, move second: X): Pair(Y, X) = { Pair { first: first, second: second } }
  let take_first(move self)(): Y = { self.first }
}

let main(): i32 = {
  let pair = Pair.new(42, true)
  pair.take_first()
}
