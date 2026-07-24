let main(): i32 = {
  let outer = 40
  let local: i32 = do {
    if outer == 40 { return(outer) }
    0
  }
  local + 2
}
