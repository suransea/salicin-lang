use core.Result

use core.effects.{Throws, Unsafe}

let read(fail: bool): i32 with(Throws(bool), Unsafe) = {
  if fail { throw(true) }
  42
}

let main(): i32 = {
  let result: Result(i32, bool) = try { unsafe { read(false) } }
  result ?? 0
}
