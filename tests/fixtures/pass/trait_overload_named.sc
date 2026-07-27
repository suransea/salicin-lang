let select = trait {
  let pick(self: borrow(self))(left: i32): i32
  let pick(self: borrow(self))(right: i32): i32
  let make(left: i32): i32
  let make(right: i32): i32
}

let counter = struct { value: i32 }

extend(counter, select) {
  let pick(self: borrow(self))(left: i32): i32 = { self.value + left }
  let pick(self: borrow(self))(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): i32 = { left }
  let make(right: i32): i32 = { right + 1 }
}

let main(): i32 = {
  counter { value: 0 }.pick(right: 20) + counter.make(right: 20)
}

test("trait_overload_named.sc") {
  main() == 42
}
