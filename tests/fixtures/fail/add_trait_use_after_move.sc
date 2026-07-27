let add = core.ops.add

let number = struct { value: i32 }

extend(number, add(number)) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}

let main(): i32 = {
  let left = number { value: 40 }
  let answer = left + number { value: 2 }
  left.value
}
