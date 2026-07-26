let pass(comptime r: region)(value: borrow(r)(i32)): borrow(r)(i32) = { value }

let bad(comptime r: region)(seed: borrow(r)(i32)): borrow(r)(i32) = {
  let local = seed
  let reference: borrow(i32) = borrow(local)
  pass(reference)
}

let main(): i32 = { 42 }
