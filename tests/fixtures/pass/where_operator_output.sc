use std.ops.Add

let twice(T: type)(copy value: T): T
where T: Add(T, Output = T),
      T: Copy = {
  let left = value
  let right = value
  left + right
}

let main(): i32 = { twice(21) }
