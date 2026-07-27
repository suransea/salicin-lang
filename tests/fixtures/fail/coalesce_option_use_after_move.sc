let option = core.option

let main(): i32 = {
  let value = option(i32).some(42)
  let answer = value ?? 0
  match value
    { some(item) -> item }
    { none -> answer }
}
