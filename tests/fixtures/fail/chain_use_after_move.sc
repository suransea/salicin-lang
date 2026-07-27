let option = core.option

let boxed = struct { value: i32 }

let main(): i32 = {
  let boxed = option(boxed).some(boxed { value: 42 })
  let answer = boxed?.value ?? 0
  answer + (boxed?.value ?? 0)
}
