let Counter = struct { value: i32 }

let reset(counter: borrow(mut)(Counter)): i32 = {
  counter.value = 42
  counter.value
}

let main(): i32 = { reset(Counter { value: 0 }) }

test("temporary_mut_borrow_argument.sc") {
  main() == 42
}
