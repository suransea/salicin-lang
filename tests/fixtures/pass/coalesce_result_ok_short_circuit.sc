use core.Result

let fallback(borrow(mut) count: i32): i32 = {
  count = count + 1
  0
}

let main(): i32 = {
  let mut count = 0
  let answer = Result(bool)(i32).Ok(42) ?? fallback(count)
  if count == 0 { answer } else { 0 }
}
