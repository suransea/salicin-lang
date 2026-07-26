let option = std.option

let inner = struct { answer: i32 }
let middle = struct { inner: inner }
let outer = struct { middle: middle }

let main(): i32 = {
  option(outer).some(outer { middle: middle { inner: inner { answer: 42 } } })?.middle?.inner?.answer ?? 0
}

test("chain_consecutive_fields.sc") {
  main() == 42
}
