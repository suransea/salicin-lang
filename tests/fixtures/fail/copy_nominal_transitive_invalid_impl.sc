let leaf = struct { value: i32 }

let branch = struct { leaf: leaf }

let tree = struct { branch: branch }

extend(branch, copyable) {}

extend(tree, copyable) {}

let main(): i32 = { 42 }
