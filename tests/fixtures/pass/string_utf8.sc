let greeting: string = "柳"

let runtime_text(): string = {
  "salicin"
}

let main(): i32 = {
  let text = runtime_text()
  42
}

test("string_utf8.sc") {
  main() == 42
}
