let Cell(T: type) = struct { value: T }
let Family(T: type): type = Cell(T)

let main(value: Family): i32 = { 0 }
