let Number = struct(value: i32)

extend Number {
  let add(borrow self)(borrow other: Number): i32 = self.value + other.value
}

let main(): i32 = Number(20).add(Number(22))
