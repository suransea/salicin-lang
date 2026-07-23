use std.Result
use std.effect.Throws
use std.flow.Raise

let Stored = enum {
  Value(i32),
  Failure(bool),
}

extend Stored: Raise {
  let Output = i32
  let Error = bool

  let raise(move self): i32 with(Throws(bool)) = {
    self match {
      Value(value) => value,
      Failure(error) => throw(error),
    }
  }
}

let extract(move stored: Stored): i32 with(Throws(bool)) = {
  stored!
}

let extract_direct(move stored: Stored): i32 with(Throws(bool)) = {
  stored.raise()
}

let extract_local(): i32 with(Throws(bool)) = {
  let stored: Stored = Stored.Value(42)
  stored.raise()
}

let main(): i32 = {
  let success = try {
    extract_local()
  }!!
  let failure = try {
    extract(Stored.Failure(false))
  } ?? 0
  success + failure
}
