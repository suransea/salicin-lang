let counter = struct { value: i32 }

let main(): i32 = {
  let counter = counter { value: 40 }
  counter.value = 42
  counter.value
}
