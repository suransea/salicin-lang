let option = core.option

let main(): i32 = {
  let present = option(bool).some(false)
  if present ?? false || true { 0 } else { 42 }
}

test("coalesce_logical_or_precedence.sc") {
  main() == 42
}
