let Boxed = struct(value: i32)

extend Boxed {
  let checked(move self)(): Result(i32, bool) = { Result(i32, bool).Ok(self.value) }
}

let main(): i32 = {
  let flattened: Result(i32, bool) =
    Result(Boxed, bool).Ok(Boxed(42))?.checked()
  flattened ?? 0
}
