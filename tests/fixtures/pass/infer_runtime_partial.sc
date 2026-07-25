let choose(T: type)(first: T)(second: T): T = { second }

let main(): i32 = {
  let choose_after_zero = choose(0)
  choose_after_zero(42)
}

test("infer_runtime_partial.sc") {
  main() == 42
}
