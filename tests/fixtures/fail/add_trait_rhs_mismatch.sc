let add_operator = std.ops.add_operator

let number = struct { value: i32 }
let offset = struct { value: i32 }

extend number: add_operator(i32) {
  let output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}

let main(): i32 = { number { value: 40 } + offset { value: 2 } }
