let Option = std.Option

let choose(value: Option(i32)): i32 = {
  match value
    { Some(found) -> found }
    { None -> 2 }
}

let main(): i32 = {
  choose(Some(40)) + choose(None)
}
