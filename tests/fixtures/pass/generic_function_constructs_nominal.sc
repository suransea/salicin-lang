let Cell(T: type) = struct(value: T)

let wrap(T: type)(move value: T): Cell(T) = { Cell(T)(value) }

let main(): i32 = {
  let wrapped = wrap(i32)(42)
  wrapped.value
}
