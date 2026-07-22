let main(): i32 = {
  let fail = { () -> throw(true) }
  fail()
}
