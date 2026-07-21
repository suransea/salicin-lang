let main(): i32 = {
  let mut value = 41
  let bump = { ->
    value = value + 1
    value
  }
  bump()
}
