let Option = std.Option

let Boxed = struct { value: i32 }

let make(count: borrow(mut)(i32)): Option(Boxed) = {
  count = count + 1
  Option(Boxed).Some(Boxed { value: 42 })
}

let main(): i32 = {
  let mut count = 0
  let answer = make(count)?.value ?? 0
  if count == 1 { answer } else { 0 }
}
