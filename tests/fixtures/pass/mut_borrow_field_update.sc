let counter = struct { value: i32 }

let increment(counter: borrow(mut)(counter))(amount: i32): () = {
  counter.value = counter.value + amount
}

let main(): i32 = {
  let mut counter = counter { value: 40 }
  increment(counter)(2)
  counter.value
}

test("mut_borrow_field_update.sc") {
  main() == 42
}
