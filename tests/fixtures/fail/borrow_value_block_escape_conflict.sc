let main(): i32 = {
  let mut number = 42
  let reference: borrow i32 = do {
    let inner: borrow i32 = borrow number
    inner
  }
  number = 0
  reference
}
