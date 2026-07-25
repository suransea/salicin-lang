let Wrong(T: type): type = T

let Lend = trait {
  let Item(A: access): type
}

let Cell = struct { value: i32 }

extend Cell: Lend {
  let Item = Wrong
}

let main(): i32 = { 0 }
