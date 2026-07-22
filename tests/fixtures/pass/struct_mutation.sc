let Counter = struct { value: i32 }

let main(): i32 = {
  let mut counter = Counter { value: 40 }
  counter.value = counter.value + 2
  counter.value
}
