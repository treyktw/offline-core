//! Shared catalog of public backend functions used by the native mobile apps.
//!
//! The TypeScript backend remains authoritative. This catalog gives the
//! native core a reviewed function kind and offline-delivery policy without
//! duplicating backend business logic.

/// Backend function execution kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionKind {
    /// Reactive or one-shot read.
    Query,
    /// Transactional database write.
    Mutation,
    /// Potentially non-transactional server action.
    Action,
}

impl FunctionKind {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Action => "action",
        }
    }

    /// Parses the stable database representation.
    pub const fn parse(value: &str) -> Option<Self> {
        match value.as_bytes() {
            b"query" => Some(Self::Query),
            b"mutation" => Some(Self::Mutation),
            b"action" => Some(Self::Action),
            _ => None,
        }
    }
}

/// Whether a function may be persisted for later delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflinePolicy {
    /// Must execute while online and must not enter the durable outbox.
    OnlineOnly,
    /// Safe to retry because the server contract enforces idempotency.
    Queueable,
    /// Mobile needs offline support, but the server contract needs an
    /// idempotency key or equivalent deduplication guarantee first.
    RequiresServerIdempotency,
}

/// Repeat-safety strategy required by a queueable function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdempotencyStrategy {
    /// Function is not approved for durable offline delivery.
    None,
    /// The operation assigns authoritative state and is intrinsically safe to repeat.
    Intrinsic,
    /// One top-level argument carries the stable client identity.
    Argument(&'static str),
    /// Every item in an array has its own stable identity.
    CollectionItem {
        /// Top-level array argument.
        collection: &'static str,
        /// Identity field on every item.
        field: &'static str,
    },
    /// A root identity and per-item sequence jointly deduplicate records.
    RootAndCollectionItem {
        /// Top-level identity argument.
        root_field: &'static str,
        /// Top-level array argument.
        collection: &'static str,
        /// Per-item identity or sequence field.
        item_field: &'static str,
    },
}

/// Reviewed metadata for one public backend function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionContract {
    /// Public backend function path.
    pub name: &'static str,
    /// Execution kind.
    pub kind: FunctionKind,
    /// Durable offline-delivery policy.
    pub offline_policy: OfflinePolicy,
    /// Server deduplication shape for durable retries.
    pub idempotency: IdempotencyStrategy,
}

macro_rules! function {
    ($name:literal, $kind:ident, $policy:ident) => {
        FunctionContract {
            name: $name,
            kind: FunctionKind::$kind,
            offline_policy: OfflinePolicy::$policy,
            idempotency: IdempotencyStrategy::None,
        }
    };
    ($name:literal, $kind:ident, $policy:ident, $idempotency:expr) => {
        FunctionContract {
            name: $name,
            kind: FunctionKind::$kind,
            offline_policy: OfflinePolicy::$policy,
            idempotency: $idempotency,
        }
    };
}

/// Catalog mirrored from `contracts/functions.ts`.
///
/// Queueability is intentionally stricter than being a mutation. Only
/// operations with intrinsic repeat safety or a server-enforced deduplication
/// identity are queueable.
///
/// This is a toy catalog for the showcase. The production catalog had the same
/// shape with every function the mobile apps were allowed to call.
pub static MOBILE_FUNCTIONS: &[FunctionContract] = &[
    function!("tasks:list", Query, OnlineOnly),
    function!(
        "tasks:create",
        Mutation,
        Queueable,
        IdempotencyStrategy::Argument("clientId")
    ),
    function!(
        "tasks:complete",
        Mutation,
        Queueable,
        IdempotencyStrategy::Intrinsic
    ),
    function!(
        "tasks:logTime",
        Mutation,
        Queueable,
        IdempotencyStrategy::CollectionItem {
            collection: "entries",
            field: "entryId",
        }
    ),
    function!("tasks:addComment", Mutation, RequiresServerIdempotency),
    function!(
        "sensors:ingestReadings",
        Mutation,
        Queueable,
        IdempotencyStrategy::RootAndCollectionItem {
            root_field: "deviceId",
            collection: "readings",
            item_field: "seq",
        }
    ),
];

/// Finds reviewed metadata for a public backend function.
pub fn find_function(name: &str) -> Option<&'static FunctionContract> {
    MOBILE_FUNCTIONS
        .iter()
        .find(|contract| contract.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::{
        FunctionKind, IdempotencyStrategy, MOBILE_FUNCTIONS, OfflinePolicy, find_function,
    };

    #[test]
    fn catalog_names_are_unique() {
        let mut names = HashSet::new();
        for contract in MOBILE_FUNCTIONS {
            assert!(names.insert(contract.name), "duplicate {}", contract.name);
        }
    }

    #[test]
    fn queueable_contracts_are_writes() {
        for contract in MOBILE_FUNCTIONS {
            if contract.offline_policy == OfflinePolicy::Queueable {
                assert_ne!(contract.kind, FunctionKind::Query, "{}", contract.name);
                assert_ne!(
                    contract.idempotency,
                    IdempotencyStrategy::None,
                    "{}",
                    contract.name
                );
            }
        }
    }

    #[test]
    fn known_idempotent_create_is_queueable() {
        let contract = find_function("tasks:create").expect("contract should exist");
        assert_eq!(contract.offline_policy, OfflinePolicy::Queueable);
        assert_eq!(
            contract.idempotency,
            IdempotencyStrategy::Argument("clientId")
        );
    }

    #[test]
    fn repeat_safe_state_assignments_are_queueable() {
        let contract = find_function("tasks:complete").expect("contract should exist");
        assert_eq!(contract.offline_policy, OfflinePolicy::Queueable);
        assert_eq!(contract.idempotency, IdempotencyStrategy::Intrinsic);
    }

    #[test]
    fn append_only_writes_are_not_queueable() {
        let contract = find_function("tasks:addComment").expect("contract should exist");
        assert_eq!(
            contract.offline_policy,
            OfflinePolicy::RequiresServerIdempotency
        );
        assert_eq!(contract.idempotency, IdempotencyStrategy::None);
    }

    #[test]
    fn catalog_paths_match_mobile_typescript_boundary() {
        let source_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/functions.ts");
        let source = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_path.display()));
        let functions = source
            .split_once("export const fns = {")
            .expect("mobile function catalog should exist")
            .1;
        let typescript_names = extract_function_names(functions);
        let rust_names = MOBILE_FUNCTIONS
            .iter()
            .map(|contract| contract.name)
            .collect::<HashSet<_>>();

        assert_eq!(
            typescript_names, rust_names,
            "update the Rust contract catalog when the TypeScript mobile boundary changes"
        );
    }

    fn extract_function_names(source: &str) -> HashSet<&str> {
        source
            .split("makeFunctionReference(")
            .skip(1)
            .filter_map(|tail| {
                let quote_start = tail.find(['"', '\''])?;
                let quote = tail.as_bytes()[quote_start];
                let value = &tail[quote_start + 1..];
                let quote_end = value.as_bytes().iter().position(|byte| *byte == quote)?;
                Some(&value[..quote_end])
            })
            .collect()
    }
}
