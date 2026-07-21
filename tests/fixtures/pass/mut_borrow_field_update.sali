let Counter = struct(value: i32)

let increment(borrow(mut) counter: Counter)(amount: i32): () = {
  counter.value = counter.value + amount
}

let main(): i32 = {
  let mut counter = Counter(value: 40)
  increment(counter)(2)
  counter.value
}
