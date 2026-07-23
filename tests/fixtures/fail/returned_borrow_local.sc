let bad(R: region)(seed: borrow(R)(i32)): borrow(R)(i32) = {
  let local = seed
  borrow(local)
}

let main(): i32 = { 42 }
