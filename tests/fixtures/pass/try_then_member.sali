let Boxed = struct(value: i32)

let read(move value: Option(Boxed)): Option(i32) = value.try.value

let main(): i32 = read(Option(Boxed).Some(Boxed(42))) ?? 0
