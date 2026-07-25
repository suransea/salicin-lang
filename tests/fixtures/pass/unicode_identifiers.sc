let 加法(左: i32)(右: i32): i32 = {
  左 + 右
}

let café = 40

let main(): i32 = {
  let café = café
  加法(café)(2)
}

test("unicode_identifiers.sc") {
  main() == 42
}
