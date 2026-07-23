let bad('a: region)(seed: borrow('a)(i32)): borrow('a)(i32) = {
  let local = seed
  borrow(local)
}

let main(): i32 = { 42 }
