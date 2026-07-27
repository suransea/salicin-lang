let a = struct { value: i32 }

extend(a) {
  let answer = 41
}

extend(a) {
  let answer(): i32 = { 42 }
}

let main(): i32 = { 0 }
