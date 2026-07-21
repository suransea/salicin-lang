let Number = struct(value: i32)

extend Number {
  let plus(borrow self)(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let add_last = Option(Number).Some(Number(40))?.plus(1)
  add_last(1) ?? 0
}
