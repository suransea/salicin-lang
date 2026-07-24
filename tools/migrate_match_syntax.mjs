import fs from "node:fs";
import path from "node:path";

function codeWords(source, wanted) {
  const positions = [];
  let state = "code";
  let blockDepth = 0;
  for (let index = 0; index < source.length; index += 1) {
    const ch = source[index];
    const next = source[index + 1];
    if (state === "line") {
      if (ch === "\n") state = "code";
      continue;
    }
    if (state === "block") {
      if (ch === "/" && next === "*") {
        blockDepth += 1;
        index += 1;
      } else if (ch === "*" && next === "/") {
        blockDepth -= 1;
        index += 1;
        if (blockDepth === 0) state = "code";
      }
      continue;
    }
    if (state === "string") {
      if (ch === "\\") index += 1;
      else if (ch === '"') state = "code";
      continue;
    }
    if (state === "char") {
      if (ch === "\\") index += 1;
      else if (ch === "'") state = "code";
      continue;
    }
    if (ch === "/" && next === "/") {
      state = "line";
      index += 1;
      continue;
    }
    if (ch === "/" && next === "*") {
      state = "block";
      blockDepth = 1;
      index += 1;
      continue;
    }
    if (ch === '"') {
      state = "string";
      continue;
    }
    if (ch === "'" && source[index + 2] === "'") {
      state = "char";
      continue;
    }
    if (/[A-Za-z_]/u.test(ch)) {
      let end = index + 1;
      while (end < source.length && /[A-Za-z0-9_]/u.test(source[end])) end += 1;
      if (source.slice(index, end) === wanted) positions.push(index);
      index = end - 1;
    }
  }
  return positions;
}

function skipSpace(source, index) {
  while (index < source.length && /\s/u.test(source[index])) index += 1;
  return index;
}

function matchingBrace(source, open) {
  let depth = 0;
  let state = "code";
  let blockDepth = 0;
  for (let index = open; index < source.length; index += 1) {
    const ch = source[index];
    const next = source[index + 1];
    if (state === "line") {
      if (ch === "\n") state = "code";
      continue;
    }
    if (state === "block") {
      if (ch === "/" && next === "*") {
        blockDepth += 1;
        index += 1;
      } else if (ch === "*" && next === "/") {
        blockDepth -= 1;
        index += 1;
        if (blockDepth === 0) state = "code";
      }
      continue;
    }
    if (state === "string") {
      if (ch === "\\") index += 1;
      else if (ch === '"') state = "code";
      continue;
    }
    if (ch === "/" && next === "/") {
      state = "line";
      index += 1;
    } else if (ch === "/" && next === "*") {
      state = "block";
      blockDepth = 1;
      index += 1;
    } else if (ch === '"') {
      state = "string";
    } else if (ch === "{") {
      depth += 1;
    } else if (ch === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error(`unclosed match body at byte ${open}`);
}

function expressionStart(source, matchStart) {
  let index = matchStart - 1;
  while (index >= 0 && /[ \t]/u.test(source[index])) index -= 1;
  let paren = 0;
  let bracket = 0;
  let brace = 0;
  for (; index >= 0; index -= 1) {
    const ch = source[index];
    if (ch === ")") paren += 1;
    else if (ch === "(") {
      if (paren > 0) paren -= 1;
      else if (bracket === 0 && brace === 0) break;
    } else if (ch === "]") bracket += 1;
    else if (ch === "[") {
      if (bracket > 0) bracket -= 1;
      else if (paren === 0 && brace === 0) break;
    } else if (ch === "}") brace += 1;
    else if (ch === "{") {
      if (brace > 0) brace -= 1;
      else if (paren === 0 && bracket === 0) break;
    } else if (
      paren === 0 &&
      bracket === 0 &&
      brace === 0 &&
      (ch === "\n" ||
        ch === ";" ||
        ch === "," ||
        ch === "=" ||
        (ch === "-" && source[index + 1] === ">"))
    ) {
      break;
    }
  }
  let start = index + 1;
  if (source[start] === ">") start += 1;
  if (source[index] === "-" && source[index + 1] === ">") start = index + 2;
  return skipSpace(source, start);
}

function topLevelParts(source) {
  const parts = [];
  let start = 0;
  let paren = 0;
  let bracket = 0;
  let brace = 0;
  let state = "code";
  let blockDepth = 0;
  for (let index = 0; index < source.length; index += 1) {
    const ch = source[index];
    const next = source[index + 1];
    if (state === "line") {
      if (ch === "\n") state = "code";
      continue;
    }
    if (state === "block") {
      if (ch === "/" && next === "*") {
        blockDepth += 1;
        index += 1;
      } else if (ch === "*" && next === "/") {
        blockDepth -= 1;
        index += 1;
        if (blockDepth === 0) state = "code";
      }
      continue;
    }
    if (state === "string") {
      if (ch === "\\") index += 1;
      else if (ch === '"') state = "code";
      continue;
    }
    if (ch === "/" && next === "/") {
      state = "line";
      index += 1;
    } else if (ch === "/" && next === "*") {
      state = "block";
      blockDepth = 1;
      index += 1;
    } else if (ch === '"') {
      state = "string";
    } else if (ch === "(") paren += 1;
    else if (ch === ")") paren -= 1;
    else if (ch === "[") bracket += 1;
    else if (ch === "]") bracket -= 1;
    else if (ch === "{") brace += 1;
    else if (ch === "}") brace -= 1;
    else if (ch === "," && paren === 0 && bracket === 0 && brace === 0) {
      parts.push(source.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(source.slice(start));
  return parts.filter((part) => part.trim().length > 0);
}

function armArrow(arm) {
  let paren = 0;
  let bracket = 0;
  let brace = 0;
  for (let index = 0; index + 1 < arm.length; index += 1) {
    const ch = arm[index];
    if (ch === "(") paren += 1;
    else if (ch === ")") paren -= 1;
    else if (ch === "[") bracket += 1;
    else if (ch === "]") bracket -= 1;
    else if (ch === "{") brace += 1;
    else if (ch === "}") brace -= 1;
    else if (
      ch === "=" &&
      arm[index + 1] === ">" &&
      paren === 0 &&
      bracket === 0 &&
      brace === 0
    ) {
      return index;
    }
  }
  throw new Error(`match arm has no top-level =>: ${arm.trim()}`);
}

function migrateOne(source) {
  const candidates = codeWords(source, "match").filter((position) => {
    const brace = skipSpace(source, position + "match".length);
    return source[brace] === "{";
  });
  if (candidates.length === 0) return null;
  const matchStart = candidates.at(-1);
  const bodyOpen = skipSpace(source, matchStart + "match".length);
  const bodyClose = matchingBrace(source, bodyOpen);
  const start = expressionStart(source, matchStart);
  const input = source.slice(start, matchStart).trimEnd();
  if (input.length === 0) throw new Error(`empty postfix match input at byte ${matchStart}`);
  const lineStart = source.lastIndexOf("\n", start - 1) + 1;
  const indent = source.slice(lineStart, start).match(/^[ \t]*/u)?.[0] ?? "";
  const arms = topLevelParts(source.slice(bodyOpen + 1, bodyClose)).map((arm) => {
    const arrow = armArrow(arm);
    const pattern = arm.slice(0, arrow).trim();
    const body = arm.slice(arrow + 2).trim();
    return `${indent}  { ${pattern} -> ${body} }`;
  });
  const replacement = `match ${input}\n${arms.join("\n")}`;
  return source.slice(0, start) + replacement + source.slice(bodyClose + 1);
}

function migrate(source) {
  let current = source;
  while (true) {
    const next = migrateOne(current);
    if (next === null) return current;
    current = next;
  }
}

const roots = process.argv.slice(2);
if (roots.length === 0) throw new Error("pass one or more .sc files or directories");
const files = [];
function collect(target) {
  const stat = fs.statSync(target);
  if (stat.isDirectory()) {
    for (const entry of fs.readdirSync(target)) collect(path.join(target, entry));
  } else if (target.endsWith(".sc")) {
    files.push(target);
  }
}
for (const root of roots) collect(root);
for (const file of files) {
  const source = fs.readFileSync(file, "utf8");
  const migrated = migrate(source);
  if (migrated !== source) {
    fs.writeFileSync(file, migrated);
    process.stdout.write(`${file}\n`);
  }
}
