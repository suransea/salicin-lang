let wrong(comptime t: type): type = t

let lend = trait {
  let item(comptime a: access): type
}

let cell = struct { value: i32 }

extend(cell, lend) {
  let item = wrong
}

let main(): i32 = { 0 }
