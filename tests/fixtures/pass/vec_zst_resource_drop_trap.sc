use alloc.vec.Vec

let Bomb = struct {}

extend Bomb: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    raw_trap()
  }
  }}

let main(): i32 = {
  let mut values: Vec(Bomb) = Vec(Bomb).new()
  values.push(Bomb {})
  0
}
