let Counter = struct { value: i32 }

let main(): i32 = {
  let counter = Counter { value: 40 }
  counter.value = 42
  counter.value
}
