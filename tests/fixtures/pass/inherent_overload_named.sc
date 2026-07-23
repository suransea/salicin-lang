let Counter = struct { value: i32 }

extend Counter {
  let add(self: borrow(Self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(Self))(right: i32): i32 = { self.value + right + 1 }

  let make(left: i32): Counter = { Counter { value: left } }
  let make(right: i32): Counter = { Counter { value: right + 1 } }
}

let main(): i32 = {
  let counter = Counter.make(right: 19)
  counter.add(right: 21)
}
