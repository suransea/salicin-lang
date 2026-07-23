let pass(R: region)(value: borrow(R)(i32)): borrow(R)(i32) = { value }

let bad(R: region)(seed: borrow(R)(i32)): borrow(R)(i32) = {
  let local = seed
  let reference: borrow(i32) = borrow(local)
  pass(reference)
}

let main(): i32 = { 42 }
