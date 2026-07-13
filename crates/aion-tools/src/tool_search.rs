use async_trait::async_trait;
use serde_json::{Value, json};

use aion_protocol::events::ToolCategory;
use aion_types::tool::{JsonSchema, ToolDef, ToolResult};

use crate::Tool;
use crate::registry::{LoadedSchemaSet, mark_schemas_loaded};

/// Built-in tool that searches for deferred tools and loads their full schema.
/// Core tool (never deferred itself) — always available to the LLM.
pub struct ToolSearchTool {
    /// Snapshot of all tool definitions (taken at construction time).
    tool_defs: Vec<ToolDef>,
    /// Shared promoted-schema set from the owning registry. Matched tools are
    /// inserted here so subsequent requests declare their full schema —
    /// schema-constrained providers cannot use a schema that only exists as
    /// tool-result text.
    loaded_schemas: Option<LoadedSchemaSet>,
}

impl ToolSearchTool {
    pub fn new(tool_defs: Vec<ToolDef>) -> Self {
        Self {
            tool_defs,
            loaded_schemas: None,
        }
    }

    /// Construct with the registry's promoted-schema handle so successful
    /// searches promote the matched deferred tools to full declaration.
    pub fn with_loaded_schemas(tool_defs: Vec<ToolDef>, loaded_schemas: LoadedSchemaSet) -> Self {
        Self {
            tool_defs,
            loaded_schemas: Some(loaded_schemas),
        }
    }

    /// Comma-separated names of all deferred tools in the snapshot, for the
    /// miss message — stops models from retrying free-text queries blindly.
    fn deferred_names(&self) -> String {
        self.tool_defs
            .iter()
            .filter(|d| d.deferred)
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "ToolSearch"
    }

    fn description(&self) -> &str {
        "Search for deferred tools and load their full schema. \
         Use this before calling any deferred tool."
    }

    fn input_schema(&self) -> JsonSchema {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Tool name or keyword to search for"
                }
            },
            "required": ["query"]
        })
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value) -> ToolResult {
        let query = input["query"].as_str().unwrap_or("");
        if query.is_empty() {
            return ToolResult {
                content: "Error: query is required".to_string(),
                is_error: true,
            };
        }

        let query_lower = query.to_lowercase();
        let matches: Vec<Value> = self
            .tool_defs
            .iter()
            .filter(|d| d.deferred)
            .filter(|d| {
                d.name.to_lowercase().contains(&query_lower) || d.description.to_lowercase().contains(&query_lower)
            })
            .map(|d| {
                json!({
                    "name": d.name,
                    "description": d.description,
                    "parameters": d.input_schema
                })
            })
            .collect();

        if matches.is_empty() {
            let deferred = self.deferred_names();
            let deferred_line = if deferred.is_empty() {
                "There are no deferred tools in this session at all — never call ToolSearch again.".to_string()
            } else {
                format!("The only deferred tools in this session are: {deferred}.")
            };
            return ToolResult {
                content: format!(
                    "No deferred tools matching \"{query}\" found. {deferred_line} \
                     Every other tool already appears in your available tools list with its \
                     full parameters — call those directly by name instead of searching. \
                     Do not use ToolSearch to look for skills: invoke skills with the Skill tool."
                ),
                is_error: false,
            };
        }

        // Promote matched tools: from the next request on they are declared
        // with their full schema, so schema-constrained providers can emit
        // real arguments instead of being forced to `{}` by the stub.
        if let Some(set) = &self.loaded_schemas {
            mark_schemas_loaded(set, matches.iter().filter_map(|m| m["name"].as_str()));
        }

        ToolResult {
            content: serde_json::to_string_pretty(&matches).unwrap_or_default(),
            is_error: false,
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Info
    }
}

#[cfg(test)]
#[path = "tool_search_test.rs"]
mod tool_search_test;
