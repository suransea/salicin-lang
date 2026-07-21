let main(): i32 = {
  let mut value = 40
  let mut bump = {
    value = value + 1
    value
  }
  bump()
  bump()
}
