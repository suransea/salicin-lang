let Maybe(T: type) = enum {
  Some(T),
  None,
}

extend(T: type) Maybe(T) {
  let unwrap_or(move self)(move fallback: T): T = { self match {
    Some(value) => value,
    None => fallback,
  }
  }
}

let main(): i32 = {
  let value = Maybe.Some(42)
  value.unwrap_or(0)
}
