extend(A: access, T: type) Ptr(A)(T) {
  let identity(self)(): Ptr(A)(T) = { self }
}

let main(): i32 = { 0 }
