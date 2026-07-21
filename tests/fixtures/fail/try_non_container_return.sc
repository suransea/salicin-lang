let unwrap(move value: Option(i32)): i32 = value.try

let main(): i32 = unwrap(Option(i32).Some(42))
