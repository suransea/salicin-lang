let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T)
where T: Missing {
  let take(move self)(): T = self.value
}

let main(): i32 = 0
