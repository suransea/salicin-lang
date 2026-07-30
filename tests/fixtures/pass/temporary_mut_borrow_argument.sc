let counter = struct { value: i32 }

let reset(counter: borrow(mut)(counter)): i32 = {
  counter.value = 42
  counter.value
}

let main(): i32 = { reset(counter { value: 0 }) }

test("temporary_mut_borrow_argument.sc") {
  std.test.assert(main() == 42)
}
