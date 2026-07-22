let Convert(To: type) = trait {
  let convert(borrow self)(): To
}

let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T): Convert(i32) {
  let convert(borrow self)(): i32 = { 42 }
}

extend(T: type) Cell(T): Convert(i64) {
  let convert(borrow self)(): i64 = { 42 }
}

let main(): i32 = {
  let cell = Cell { value: true }
  42
}
