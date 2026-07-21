let fallback(borrow(mut) count: i32): i32 = {
  count = count + 1
  42
}

let main(): i32 = {
  let mut count = 0
  let answer = Option(i32).None ?? fallback(count)
  if count == 1 { answer } else { 0 }
}
