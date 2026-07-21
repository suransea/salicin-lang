let main(): i32 = {
  let mut value = 40
  let read = { -> value }
  value = 42
  read()
}
