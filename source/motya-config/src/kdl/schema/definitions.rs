//! # KDL Schema Definition System
//!
//! This module defines the data structures used to describe the expected shape, validation rules,
//! and autocompletion logic for the KDL configuration files.
//!
//! ## Architecture Overview
//!
//! Unlike a simple static schema, this system is designed for **Context-Awareness** and **Extensibility**:
//!
//! 1.  **Serialization-First**: All types derive `Serialize` and `Deserialize`. This allows the schema
//!     to be generated at runtime, serialized to JSON, and embedded into WASM plugins or read by the LSP
//!     without recompiling the parser.
//! 2.  **Polymorphism**: The system supports KDL nodes that can take multiple forms (via Rust Enums)
//!     or merge multiple definitions (via `flatten`).
//! 3.  **The Catalog Mechanism (Mixins)**: This is the core feature for extensibility. It allows
//!     properties of a node to depend on the *value* of an argument.
//!     *   *Example:* `filter "my.plugin" key="value"`.
//!     *   The LSP sees "my.plugin", looks it up in the "filters" Catalog, and learns that `key`
//!         is a required property, even though the core parser knows nothing about it.
//!
//! ## Usage
//!
//! - **LSP Server**: Consumes these structures to provide completions, signature help, and diagnostics.
//! - **Parser Macros**: Implement `GetSchema` for Rust structs to automatically generate this schema.
//! - **WASM Plugins**: Export `CatalogItemDefinition` (as JSON) to register new functionality dynamically.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::kdl::schema::schema_context::SchemaContext;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocEntry {
    pub lang: Cow<'static, str>,
    pub text: Cow<'static, str>,
}

// =============================================================================
// 1. PRIMITIVES & VALUE KINDS
// =============================================================================

/// Defines the expected type of a value for an Argument or a Property.
///
/// This enum drives validation (checking types), formatting (icons in IDE), and
/// the dynamic logic for extending available properties (Mixins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    /// A generic string value.
    String,
    /// An integer number.
    Int,
    /// A floating-point number.
    Float,
    /// A boolean value (`true` or `false`).
    Bool,
    /// A value restricted to a specific set of static strings.
    ///
    /// This is typically used for Rust enums with a fixed number of variants
    /// that are parsed from strings (e.g., `RoutingMode`).
    ///
    /// # LSP Behavior
    /// - **Autocompletion**: Displays a dropdown list containing only the strings in the `Vec`.
    /// - **Validation**: Marks any value not present in the list as an error.
    ///
    /// # Example
    /// For `RoutingMode`, the list would be `["exact", "prefix"]`.
    Enum(Vec<String>),
    /// A string value with a semantic "hint" about its content.
    ///
    /// Unlike a generic `String`, a `TypedString` tells the LSP that the text
    /// follows a specific format or protocol. The `String` argument contains
    /// the name of the type (e.g., "path", "duration", "uri").
    ///
    /// # LSP Behavior
    /// - **Validation**: The LSP can run specialized regex or logic checks (e.g.,
    ///   verifying that a "duration" follows the ISO-8601 or humantime format).
    /// - **UI Helpers**: Can trigger specialized IDE features, such as a file
    ///   picker for "path" or a link-opener for "uri".
    ///
    /// # Common Labels
    /// - `"path"`: File system paths (absolute or relative).
    /// - `"duration"`: Time intervals (e.g., "5s", "10m").
    /// - `"uri"`: Network identifiers/URLs.
    /// - `"socket-addr"`: IP addresses with ports.
    TypedString(String),
    /// A reference to an entity in a dynamic registry (Catalog).
    ///
    /// **This is the primary mechanism for Plugins and Dynamic Configuration.**
    ///
    /// # Logic for LSP
    /// When an LSP encounters an argument/property of this kind:
    /// 1. It extracts the string value provided by the user (e.g., "motya.filters.block-cidr").
    /// 2. It looks up this ID in the internal registry named `name`.
    /// 3. If found, it **mixes in** the properties defined by that catalog item into the
    ///    current node's allowed properties context.
    ///
    /// # Example
    /// ```text
    /// filter <id>
    /// ```
    /// If `<id>` has kind `Catalog { name: "filters" }`, selecting a specific filter
    /// will auto-complete the specific parameters required by that filter.
    Catalog {
        /// The name of the registry to look up. E.g., "filters", "storages", "plugins".
        name: String,
    },
}

/// Defines how a KDL Node is identified and matched against this schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeNameMatcher {
    /// The node has a fixed, static name (keyword).
    ///
    /// # Example
    /// `file-server { ... }`
    ///
    /// # LSP Behavior
    /// - **Completion**: Suggests this keyword when typing in the parent block.
    /// - **Validation**: Matches exactly against the node name.
    Keyword(String),

    /// The node name is user-defined (variable), usually acting as an identifier or key.
    ///
    /// # Example
    /// `service "my-auth-service" { ... }`
    /// Here, the schema describes a "service" context, but the actual KDL node name
    /// is "my-auth-service".
    ///
    /// # LSP Behavior
    /// - **Completion**: Does not suggest a fixed string. May suggest a snippet or placeholder.
    /// - **Display**: Uses `label` to describe the node in UI (e.g., `<service-name>`).
    Variable {
        /// A human-readable label for the variable name (e.g., "service-name", "backend-id").
        label: String,
    },
}

// =============================================================================
// 2. ARGS & PROPS
// =============================================================================

/// Describes a positional argument of a KDL node.
///
/// Syntax: `node <arg1> <arg2>`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgSchema {
    /// The name of the argument. Used for "Signature Help" in IDEs.
    pub name: String,

    /// Documentation string (Markdown supported).
    /// Displayed on hover.
    pub description: Cow<'static, [DocEntry]>,

    /// The expected type of the argument.
    /// If this is `ValueKind::Catalog`, the value of this argument determines allowed properties.
    pub kind: ValueKind,

    /// Whether this argument must be present.
    pub required: bool,

    /// The default value (informative only, used for docs).
    pub default: Option<String>,
}

/// Describes a named property of a KDL node.
///
/// Syntax: `node key="value"`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropSchema {
    /// The property key.
    pub name: String,

    /// Documentation string (Markdown supported).
    pub description: Cow<'static, [DocEntry]>,

    /// The expected type of the value.
    pub kind: ValueKind,

    /// Whether this property is mandatory.
    pub required: bool,

    /// The default value (informative only, used for docs).
    pub default: Option<String>,
}

// =============================================================================
// 3. NODE STRUCTURE
// =============================================================================

/// The complete schema definition for a KDL Node.
///
/// This structure links the Node's identity (Matcher) with its content (Args, Props, Children).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSchema {
    /// How to identify this node (Keyword or Variable).
    pub matcher: NodeNameMatcher,

    /// Main documentation for the node (Markdown supported).
    pub description: Cow<'static, [DocEntry]>,

    /// A list of usage examples or snippets.
    ///
    /// # LSP Behavior
    /// Can be used to provide "Pattern Suggestions" in autocompletion.
    /// E.g., inserting a full boilerplate for a complex node.
    #[serde(default)]
    pub examples: Vec<String>,

    /// Positional arguments definition.
    #[serde(default)]
    pub args: Vec<ArgSchema>,

    /// *Static* named properties definition.
    ///
    /// **Note:** The actual available properties during editing may be a superset of this list
    /// if dynamic properties are mixed in via `Catalog` arguments.
    #[serde(default)]
    pub props: Vec<PropSchema>,

    /// Rules for the children block `{ ... }`.
    pub children: ChildrenSchema,
}

/// Defines the rules for valid child nodes within a KDL block `{ ... }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChildrenSchema {
    /// Represents a leaf node. No children block is allowed.
    None,

    /// A strictly defined list of allowed child nodes.
    ///
    /// This is used for structural objects (Structs) or Polymorphic choices (Enums).
    ///
    /// # Logic
    /// The LSP allows autocompletion only for the nodes defined in this list.
    /// If multiple schemas match (e.g., due to flattening/enums), all their properties are valid candidates.
    Fixed(Vec<NodeSchema>),

    /// A dynamic list of nodes that share a common schema.
    ///
    /// This is used for collections like `Vec<T>` or `Map<String, T>`, where the user
    /// can repeat the same type of node multiple times.
    Dynamic(Box<NodeSchema>),
    Recursive(String), 
}

// =============================================================================
// 4. EXTERNAL DEFINITIONS (PLUGINS / CATALOGS)
// =============================================================================

/// Represents a definition loaded from an external source (Plugin/WASM/Macro).
///
/// This structure is not a KDL Node itself, but a description of a "Business Logic Entity"
/// (like a specific Filter implementation) that KDL refers to.
///
/// These definitions populate the `Catalog` registries in the LSP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItemDefinition {
    /// The unique identifier of the item.
    /// E.g., "motya.request.upsert-header".
    pub name: String,

    /// Documentation for the specific implementation.
    pub description: Cow<'static, [DocEntry]>,

    /// The list of properties that this item requires/supports.
    ///
    /// # LSP Behavior
    /// If this item is selected in a KDL node (via a Catalog argument),
    /// these properties are added to the autocompletion list and validation rules
    /// for that KDL node.
    #[serde(default)]
    pub props: Vec<PropSchema>,

    /// (Optional) Positional arguments specific to this item.
    /// Allows overriding or extending the node's base arguments.
    #[serde(default)]
    pub args: Vec<ArgSchema>,
}

// =============================================================================
// 5. TRAIT FOR PARSER INTEGRATION
// =============================================================================

/// A trait for Rust types that can describe their own KDL schema.
///
/// This should be implemented automatically via macros (e.g., `motya_macro`).
/// It allows recursive generation of the full schema tree from the root configuration struct.
pub trait GetSchema {
    /// Returns a list of possible schema definitions for this type.
    ///
    /// # Why a Vector?
    /// A single Rust type might map to multiple valid KDL node shapes due to:
    /// 1. **Polymorphism (Enums):** `ServiceMode` might be `FileServer` OR `Connectors`.
    /// 2. **Flattening:** A struct might include fields from another struct inline.
    fn schemas(ctx: &mut SchemaContext) -> Vec<NodeSchema>;
}

// --- Blanket Implementations ---

impl<T: GetSchema> GetSchema for Option<T> {
    fn schemas(ctx: &mut SchemaContext) -> Vec<NodeSchema> {
        // Option<T> affects the `required` field at the parsing logic level,
        // but the intrinsic schema of the node T remains the same.
        T::schemas(ctx)
    }
}

impl<T: GetSchema> GetSchema for Vec<T> {
    fn schemas(ctx: &mut SchemaContext) -> Vec<NodeSchema> {
        T::schemas(ctx)
    }
}

impl<T: GetSchema> GetSchema for Box<T> {
    fn schemas(ctx: &mut SchemaContext) -> Vec<NodeSchema> {
        T::schemas(ctx)
    }
}
