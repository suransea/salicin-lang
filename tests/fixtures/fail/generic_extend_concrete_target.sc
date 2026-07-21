let Cell(T: type) = struct(value: T)

extend(T: type) Cell(i32) {
  let invalid(borrow self)(): i32 = { 0 }
}

let main(): i32 = { 0 }
