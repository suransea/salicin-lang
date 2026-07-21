let Boxed = struct(answer: bool)

let main(): i32 = {
  let answer = Result(Boxed, bool).Ok(Boxed(true))?.answer
  if answer ?? false { 42 } else { 0 }
}
