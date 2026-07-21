let Counter = struct(value: i32)

let reset(borrow(mut) counter: Counter): i32 = {
  counter.value = 42
  counter.value
}

let main(): i32 = reset(Counter(0))
