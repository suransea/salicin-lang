let string_writer = alloc.string.string_writer

/// Formats the stable output consumed by both the CLI and its acceptance test.
pub let render(value: catalog.summary): core.string.string = {
  let mut writer = string_writer.new()
  "items=".display(writer)
  value.count.display(writer)
  "\ntotal=".display(writer)
  value.total.display(writer)
  "\nname_bytes=".display(writer)
  value.name_bytes.display(writer)
  "\n".display(writer)
  writer.finish()
}

test("report output is deterministic") {
  let value = catalog.summary { count: 2, total: 41, name_bytes: 4 }
  let actual = render(value)
  let expected: string = "items=2\ntotal=41\nname_bytes=4\n"
  std.test.assert(actual == expected)
}
