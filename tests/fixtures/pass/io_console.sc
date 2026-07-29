let main: with(std.io.io)(): i32 = {
  let text: string = "hello"
  let view = text.as_str()
  match std.io.println(view)
    { ok(_) -> 42 }
    { err(_) -> 1 }
}
