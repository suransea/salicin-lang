let Maybe(T: type) = enum {
  Some(T),
  None,
}

let main(): i32 = {
  let some = Maybe.Some(42)
  let none: Maybe(i32) = Maybe.None
  let from_some = match some
    { Some(value) -> value }
    { None -> 0 }
  let from_none = match none
    { Some(value) -> value }
    { None -> 0 }
  from_some + from_none
}
