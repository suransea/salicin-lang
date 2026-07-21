let read(borrow value: i32): i32 = { value }

let set(borrow(mut) value: i32): () = {
  value = 22
}

let main(): i32 = {
  let mut values = [20, 0]
  let first = read(values[0])
  set(values[1])
  let alias = borrow values[0]
  values[1] = 22
  first + alias + values[1] - 20
}
