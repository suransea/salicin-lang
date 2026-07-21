let Leaf = struct(value: i32)

let Branch = struct(leaf: Leaf)

let Tree = struct(branch: Branch)

extend Branch: Copy {}

extend Tree: Copy {}

let main(): i32 = 42
