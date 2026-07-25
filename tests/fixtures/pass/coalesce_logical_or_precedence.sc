let Option = std.Option

let main(): i32 = {
  let present = Option(bool).Some(false)
  if present ?? false || true { 0 } else { 42 }
}

test("coalesce_logical_or_precedence.sc") {
  main() == 42
}
