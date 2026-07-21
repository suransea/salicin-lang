let Number = struct(value: i32)

extend Number {
  let make(value: i32): Number = Number(value)
}

let main(): i32 = {
  let make = Number.make
  make(42).value
}
