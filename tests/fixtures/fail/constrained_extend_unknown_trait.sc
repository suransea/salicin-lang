let cell(comptime t: type) = struct { value: t }

extend(cell(t))
where t: missing {
  let take(move self)(): t = { self.value }
}

let main(): i32 = { 0 }
