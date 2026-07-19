let Maybe(T: type) = enum {
  Some(T),
  None,
}

let unwrap(move value: Maybe(i32)): i32 = value match {
  Some(item) => item,
  None => 0,
}

let main(): i32 = unwrap(Maybe(i32).Some(42))
