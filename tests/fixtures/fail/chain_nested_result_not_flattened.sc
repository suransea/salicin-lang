let result = std.result

let boxed = struct { value: i32 }

extend boxed {
  let checked(move self)(): result(bool)(i32) = { result(bool)(i32).ok(self.value) }
}

let main(): i32 = {
  let flattened: result(bool)(i32) =
    result(bool)(boxed).ok(boxed { value: 42 })?.checked()
  flattened ?? 0
}
