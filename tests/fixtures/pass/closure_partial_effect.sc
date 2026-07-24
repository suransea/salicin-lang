let Unsafe = std.effect.Unsafe

let dangerous(): i32 with(Unsafe) = { 40 }

let main(): i32 = {
  let action: (i32)(i32): i32 with(Unsafe) = {
    (left: i32)(right: i32) -> dangerous() + left + right
  }
  let pending = action(1)
  unsafe {
    pending(1)
  }
}
