use std.vec.{Vec, vec_new}

let main(): i32 = {
  let mut values: Vec(()) = vec_new()
  let mut index: u64 = 0
  while index < 100 {
    values.push(())
    index = index + 1
  }
  values.read(50)
  if values.len() == 100 && values.capacity() >= 100 {
    42
  } else {
    0
  }
}
