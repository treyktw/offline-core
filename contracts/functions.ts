// TypeScript side of the mobile function boundary.
//
// In production this file lived in the web/mobile TypeScript app and every
// entry was a typed reference into the backend's generated API. The Rust
// catalog in `crates/mobile-contracts` is tested against the names below, so
// adding a function here without reviewing its offline policy in Rust fails
// `cargo test`.

type FunctionKind = "query" | "mutation" | "action";

interface FunctionReference<Kind extends FunctionKind> {
  readonly kind: Kind;
  readonly name: string;
}

function makeFunctionReference<Kind extends FunctionKind>(
  name: string,
  kind: Kind,
): FunctionReference<Kind> {
  return { kind, name };
}

export const fns = {
  tasks: {
    list: makeFunctionReference("tasks:list", "query"),
    create: makeFunctionReference("tasks:create", "mutation"),
    complete: makeFunctionReference("tasks:complete", "mutation"),
    logTime: makeFunctionReference("tasks:logTime", "mutation"),
    addComment: makeFunctionReference("tasks:addComment", "mutation"),
  },
  sensors: {
    ingestReadings: makeFunctionReference("sensors:ingestReadings", "mutation"),
  },
} as const;
