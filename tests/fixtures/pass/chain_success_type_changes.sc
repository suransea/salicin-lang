let Result = std.Result

let Boxed = struct { answer: bool }

let main(): i32 = {
  let answer = Result(bool)(Boxed).Ok(Boxed { answer: true })?.answer
  if answer ?? false { 42 } else { 0 }
}
