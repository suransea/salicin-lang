let cell(comptime t: type) = struct { value: t }
let family(comptime t: type): type = cell(t)

let main(value: family): i32 = { 0 }
