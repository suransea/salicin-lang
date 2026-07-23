let pass('a: region)(value: borrow('a)(i32)): borrow('a)(i32) = { value }

let bad('a: region)(seed: borrow('a)(i32)): borrow('a)(i32) = {
  let local = seed
  let reference: borrow(i32) = borrow(local)
  pass(reference)
}

let main(): i32 = { 42 }
