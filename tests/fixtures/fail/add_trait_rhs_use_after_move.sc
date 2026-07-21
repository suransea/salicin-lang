use core.ops.Add

let Number = struct(value: i32)

extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { Number(self.value + rhs.value) }
}

let main(): i32 = {
  let right = Number(2)
  let answer = Number(40) + right
  right.value
}
