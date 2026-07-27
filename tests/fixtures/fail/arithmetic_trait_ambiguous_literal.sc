let mul = core.ops.mul

let number = struct { value: i32 }

extend(number, mul(i32)) {
  let output = i32
  let mul(self)(rhs: i32): i32 = { self.value * rhs }
}

extend(number, mul(i64)) {
  let output = i64
  let mul(self)(rhs: i64): i64 = { rhs * 21 }
}

let main(): i32 = {
  let answer = number { value: 21 } * 2
  42
}
