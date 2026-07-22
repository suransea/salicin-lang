use core.Result
use core.effects.Throws

let read(fail: bool): i32 with(Throws(bool)) = { if fail { throw(true) } else { 40 } }

let main(): i32 = {
  let propagated: Result(bool)(i32) = try {
    read(true) + 2
  }
  let thrown: Result(bool)(i32) = try {
    throw(true)
  }
  let success: Result(bool)(i32) = try {
    read(false) + 2
  }
  let propagation_ok = propagated match { Ok(_) => false, Err(error) => error }
  let throw_ok = thrown match { Ok(_) => false, Err(error) => error }
  let value = success match { Ok(value) => value, Err(_) => 0 }
  if propagation_ok && throw_ok && value == 42 { 42 } else { 0 }
}
