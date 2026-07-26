let add = std.ops.add

let number = struct { value: i32 }

extend number: add(number) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}

let main(): i32 = {
  let answer = number { value: 19 } + number { value: 23 }
  answer.value
}

test("add_trait_nominal_pair.sc") {
  main() == 42
}
