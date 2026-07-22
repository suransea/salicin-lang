let Invalid (T: type) = struct { next: Invalid(T) }

let main(): i32 = { 42 }
