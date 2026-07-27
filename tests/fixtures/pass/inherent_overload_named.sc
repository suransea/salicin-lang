let counter = struct { value: i32 }

extend(counter) {
  let add(self: borrow(self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(self))(right: i32): i32 = { self.value + right + 1 }

  let make(left: i32): counter = { counter { value: left } }
  let make(right: i32): counter = { counter { value: right + 1 } }
}

let main(): i32 = {
  let counter = counter.make(right: 19)
  counter.add(right: 21)
}

test("inherent_overload_named.sc") {
  main() == 42
}
