let lend = trait {
  let item(comptime a: access): type
}

let require(comptime t: type)(move value: t): ()
where t: lend(item(comptime r: region) = i32) = {}

let main(): () = {}
