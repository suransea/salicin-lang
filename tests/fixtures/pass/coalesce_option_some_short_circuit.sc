use core.Option

let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 1
  0
}

let main(): i32 = {
  let mut count = 0
  let answer = Option(i32).Some(42) ?? fallback(count)
  if count == 0 { answer } else { 0 }
}
