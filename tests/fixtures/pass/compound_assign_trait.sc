use core.ops.{AddAssign, BitXorAssign}

let Counter = struct(value: i32)

extend Counter {
  let add_assign(borrow self)(move rhs: i32): bool = { false }
}

extend Counter: AddAssign(i32) {
  let add_assign(borrow(mut) self)(move rhs: i32): () = {
    self.value += rhs
  }
}

extend Counter: BitXorAssign(i32) {
  let bit_xor_assign(borrow(mut) self)(move rhs: i32): () = {
    self.value ^= rhs
  }
}

let main(): i32 = {
  let mut counter = Counter(40)
  counter += 2
  counter ^= 0
  counter.value
}
