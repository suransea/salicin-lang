let Slice = std.Slice
let Vec = std.vec.Vec

let main(): i32 = {
  let mut values = Vec.new(i32)()
  values.push(1)
  values.push(2)
  values[1] = 40
  let borrowed = borrow(values[1])
  let from_vec = borrowed
  let mut array = [1, 2]
  let slice: borrow(mut)(Slice(i32)) = borrow(mut)(array)
  slice[0] = 2
  from_vec + slice[0]
}
