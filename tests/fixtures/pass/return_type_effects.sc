let keep(move value: Option(i32))(try): i32 = value.try

let read(pointer: Ptr(i32), fail: bool)(try(bool), unsafe): i32 = {
  if fail { throw true }
  *pointer
}

let main(): i32 = {
  let value = 42
  let optional = keep(Option(i32).None) ?? 0
  let result = unsafe { read(Ptr(borrow value), false) ?? 0 }
  optional + result
}
