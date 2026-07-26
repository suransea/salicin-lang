let bad(comptime r: region)(seed: borrow(r)(i32)): borrow(r)(i32) = {
  let local = seed
  borrow(local)
}

let main(): i32 = { 42 }
