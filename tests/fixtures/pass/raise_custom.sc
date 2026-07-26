let result = std.result
let throws = std.error.throws
let raise_operator = std.flow.raise_operator

let stored = enum {
  value(i32),
  failure(bool),
}

extend stored: raise_operator {
  let output = i32
  let error = bool

  let raise(move self): i32 with(throws(bool)) = {
    match self
      { value(value) -> value }
      { failure(error) -> throw(error) }
  }
}

let extract(move stored: stored): i32 with(throws(bool)) = {
  stored!
}

let extract_direct(move stored: stored): i32 with(throws(bool)) = {
  stored.raise()
}

let extract_local(): i32 with(throws(bool)) = {
  let stored: stored = stored.value(42)
  stored.raise()
}

let main(): i32 = {
  let success = try {
    extract_local()
  }!!
  let failure = try {
    extract(stored.failure(false))
  } ?? 0
  success + failure
}

test("raise_custom.sc") {
  main() == 42
}
