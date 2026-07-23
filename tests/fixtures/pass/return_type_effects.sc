use std.Result

use std.effect.{Throws, Unsafe}

let read(fail: bool): i32 with(Throws(bool), Unsafe) = {
  if fail { throw(true) }
  42
}

let main(): i32 = {
  let result: Result(bool)(i32) = try { unsafe { read(false) } }
  result ?? 0
}
