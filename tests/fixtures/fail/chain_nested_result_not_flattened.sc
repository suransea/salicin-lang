let Result = std.Result

let Boxed = struct { value: i32 }

extend Boxed {
  let checked(move self)(): Result(bool)(i32) = { Result(bool)(i32).Ok(self.value) }
}

let main(): i32 = {
  let flattened: Result(bool)(i32) =
    Result(bool)(Boxed).Ok(Boxed { value: 42 })?.checked()
  flattened ?? 0
}
