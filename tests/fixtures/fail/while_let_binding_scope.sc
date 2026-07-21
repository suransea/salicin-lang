let next(): Option(i32) = { None }

let main(): i32 = {
  while let Some(value) = next() {
    value
  }
  value
}
