let vec = std.vec.vec

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let mut values = vec.new(i32)()
  values.push(20)
  values.push(0)
  do {
    let slice = values.as_slice(mut)()
    let second = slice.at(mut)(1)
    second = 22
  }
  let slice = values.as_slice()
  let first = slice.at(0)
  let second = slice.at(1)
  if slice.len() == 2 {
    read(first) + read(second)
  } else {
    0
  }
}

test("slice_vec.sc") {
  main() == 42
}
