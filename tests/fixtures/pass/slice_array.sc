let Slice = std.Slice

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let values = [20, 22, 0]
  let slice: borrow(Slice(i32)) = borrow(values)
  let first = slice.at(0)
  let second = slice.at(1)
  if slice.len() == 3 {
    read(first) + read(second)
  } else {
    0
  }
}
