let result = std.result

let number = struct { value: i32 }

extend number {
  let take(move self)(): i32 = { self.value }
}

let main(): i32 = { result(bool)(number).ok(number { value: 42 })?.take() ?? 0 }

test("chain_result_method.sc") {
  main() == 42
}
