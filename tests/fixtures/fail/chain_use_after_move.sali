let Boxed = struct(value: i32)

let main(): i32 = {
  let boxed = Option(Boxed).Some(Boxed(42))
  let answer = boxed?.value ?? 0
  answer + (boxed?.value ?? 0)
}
