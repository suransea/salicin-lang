use std.vec.Vec

let main(): i32 = {
  let mut values: Vec(i32) = Vec(i32).new()
  let reserved: Vec(i32) = Vec(i32).with_capacity(8)
  let started_empty = values.is_empty()
  values.reserve(8)
  values.push(10)
  values.push(11)
  values.push(12)
  let reusable = 9
  values.push(reusable)
  values.write(1)(11)
  let removed = values.swap_remove(1)
  let score = removed + values.read(0) + values.read(1) + values.read(2)
  values.truncate(2)
  values.truncate(9)
  values.clear()
  values.clear()
  if started_empty && values.is_empty() && values.len() == 0 && values.capacity() >= 8 && reserved.len() == 0 && reserved.capacity() == 8 && reusable == 9 {
    score
  } else {
    0
  }
}
