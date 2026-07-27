let add_assign = core.ops.add_assign
let bit_xor_assign = core.ops.bit_xor_assign

let counter = struct { value: i32 }

extend(counter) {
  let add_assign(self: borrow(self))(rhs: i32): bool = { false }
}

extend(counter, add_assign(i32)) {
  let add_assign(self: borrow(mut)(self))(rhs: i32): () = {
    self.value += rhs
  }
}

extend(counter, bit_xor_assign(i32)) {
  let bit_xor_assign(self: borrow(mut)(self))(rhs: i32): () = {
    self.value ^= rhs
  }
}

let main(): i32 = {
  let mut counter = counter { value: 40 }
  counter += 2
  counter ^= 0
  counter.value
}

test("compound_assign_trait.sc") {
  main() == 42
}
