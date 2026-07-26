extend(comptime a: access, comptime t: type) ptr(a)(t) {
  let identity(self)(): ptr(a)(t) = { self }
}

let main(): i32 = { 0 }
