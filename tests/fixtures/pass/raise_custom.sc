let result = core.result
let throwing = core.error.throwing
let raise = core.flow.raise

let stored = enum {
  value(i32),
  failure(bool),
}

extend(stored, raise) {
  let output = i32
  let error = bool

  let raise(move self): i32 with(throwing(bool)) = {
    match self
      { value(value) -> value }
      { failure(error) -> throw(error) }
  }
}

let extract(move stored: stored): i32 with(throwing(bool)) = {
  stored!
}

let extract_direct(move stored: stored): i32 with(throwing(bool)) = {
  stored.raise()
}

let extract_local(): i32 with(throwing(bool)) = {
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
