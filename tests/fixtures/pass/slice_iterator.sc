let slice = core.memory.slice

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let values: array(i32)(3) = [10, 11, 21]
  let slice: borrow(slice(i32)) = borrow(values)
  let mut total = 0
  for slice.iter() { value ->
    total = total + read(value)
  }
  total
}

test("slice_iterator.sc") {
  main() == 42
}
