let copyable = std.marker.copyable
let add = std.ops.add

let number = struct { value: i32 }

extend(number, copyable) {}

extend(number, add(number)) {
  let output = number
  let add(self)(rhs: number): number = {
    number { value: self.value + rhs.value }
  }
}

let main(): i32 = {
  let left = number { value: 10 }
  let right = number { value: 11 }
  let answer = left + right
  left.value + right.value + answer.value
}

test("add_trait_copy_operands_reusable.sc") {
  main() == 42
}
