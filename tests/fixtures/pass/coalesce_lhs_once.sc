use core.Option

let make(count: borrow(mut)(i32)): Option(i32) = {
  count = count + 1
  Option(i32).Some(42)
}

let main(): i32 = {
  let mut count = 0
  let answer = make(count) ?? 0
  if count == 1 { answer } else { 0 }
}
