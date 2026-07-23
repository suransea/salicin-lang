let Counter = struct { value: i32 }

let increment(counter: borrow(mut)(Counter))(amount: i32): () = {
  counter.value = counter.value + amount
}

let main(): i32 = {
  let mut counter = Counter { value: 40 }
  increment(counter)(2)
  counter.value
}
