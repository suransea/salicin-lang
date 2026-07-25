let Option = std.Option

let main(): i32 = {
  let value: Option(i32) = Option.None
  match value
    { Some(_) -> 0 }
    { None -> 42 }
}

test("core_option_none.sc") {
  main() == 42
}
