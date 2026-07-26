let number = struct { value: i32 }

extend number {
  let make(value: i32): number = { number { value: value } }
}

let main(): i32 = {
  let make = number.make
  make(42).value
}
