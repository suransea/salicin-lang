let Slice = std.Slice

let main(): i32 = {
  let values = [1, 2]
  let slice: borrow(Slice(i32)) = borrow(values)
  slice[2]
}
