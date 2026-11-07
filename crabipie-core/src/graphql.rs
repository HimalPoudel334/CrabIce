use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionResponse {
    pub data: SchemaWrapper,
}

#[derive(Debug, Deserialize)]
pub struct SchemaWrapper {
    #[serde(rename = "__schema")]
    pub schema: GraphqlSchema,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlSchema {
    pub query_type: TypeRef,
    pub types: Vec<GraphqlType>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TypeRef {
    pub name: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlType {
    pub name: Option<String>,
    pub kind: String,
    pub fields: Option<Vec<GraphqlField>>,
    pub input_fields: Option<Vec<GraphqlField>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlField {
    pub name: String,
    // The "type" field is an object, not a string
    #[serde(rename = "type")]
    pub field_type: TypeDetail,

    #[serde(default)]
    pub args: Vec<GraphqlArg>,

    #[serde(default)]
    pub is_selected: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlArg {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: TypeDetail,

    #[serde(default)] // Default to false when parsing JSON
    pub is_selected: bool,
}

impl std::fmt::Display for GraphqlArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.arg_type)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TypeDetail {
    pub name: Option<String>,
    pub kind: String,
    // This allows for [String!]! logic
    pub of_type: Option<Box<TypeDetail>>,
}

impl TypeDetail {
    /// Recursively finds the actual name of the type,
    /// bypassing NON_NULL and LIST wrappers.
    pub fn get_base_name(&self) -> Option<&String> {
        if let Some(ref name) = self.name {
            Some(name)
        } else if let Some(ref inner) = self.of_type {
            inner.get_base_name()
        } else {
            None
        }
    }
}

impl std::fmt::Display for TypeDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind.as_str() {
            "NON_NULL" => {
                if let Some(inner) = &self.of_type {
                    write!(f, "{}!", inner)
                } else {
                    write!(f, "Unknown!")
                }
            }
            "LIST" => {
                if let Some(inner) = &self.of_type {
                    write!(f, "[{}]", inner)
                } else {
                    write!(f, "[Unknown]")
                }
            }
            // For SCALAR, OBJECT, etc., just print the name
            _ => write!(f, "{}", self.name.as_deref().unwrap_or("Unknown")),
        }
    }
}

pub fn parse_graphql_schema(json: &str) -> Result<GraphqlSchema, String> {
    let wrapper: IntrospectionResponse =
        serde_json::from_str(json).map_err(|e| format!("JSON Deserialization Error: {}", e))?;

    let mut schema = wrapper.data.schema;

    schema.types.retain(|t| {
        t.name
            .as_ref()
            .map(|name| !name.starts_with("__"))
            .unwrap_or(true)
    });

    Ok(schema)
}
pub fn generate_selection_set(
    all_types: &[GraphqlType],
    current_type: &GraphqlType,
    selected_paths: &std::collections::HashSet<String>,
    indent: usize,
    current_path: &str,
) -> String {
    let mut output = String::new();
    let spaces = "  ".repeat(indent);

    let fields = current_type
        .fields
        .as_ref()
        .or(current_type.input_fields.as_ref());

    if let Some(fields) = fields {
        for field in fields {
            let field_path = format!("{}.{}", current_path, field.name);

            // ONLY proceed if this field is directly selected
            if !selected_paths.contains(&field_path) {
                continue;
            }

            output.push_str(&format!("{}{}", spaces, field.name));

            // --- 1. Handle Arguments ---
            let active_args: Vec<_> = field
                .args
                .iter()
                .filter(|a| selected_paths.contains(&format!("{}.arg:{}", field_path, a.name)))
                .collect();

            if !active_args.is_empty() {
                output.push('(');
                let arg_strings: Vec<String> = active_args
                    .iter()
                    .map(|a| format!("{}: ${}", a.name, a.name))
                    .collect();
                output.push_str(&arg_strings.join(", "));
                output.push(')');
            }

            // --- 2. Handle Sub-selection (Nested Objects) ---
            let base_name = field.field_type.get_base_name();
            let sub_type = all_types
                .iter()
                .find(|t| t.name.as_deref() == base_name.map(|s| s.as_str()));

            if let Some(st) = sub_type {
                // Check if any children of this field are selected (excluding args)
                let has_selected_children = selected_paths
                    .iter()
                    .any(|p| p.starts_with(&format!("{}.", field_path)) && !p.contains(".arg:"));

                if has_selected_children {
                    output.push_str(" {\n");
                    output.push_str(&generate_selection_set(
                        all_types,
                        st,
                        selected_paths,
                        indent + 1,
                        &field_path,
                    ));
                    output.push_str(&format!("{}}}\n", spaces));
                } else {
                    output.push('\n');
                }
            } else {
                output.push('\n');
            }
        }
    }
    output
}

pub fn generate_variables(
    all_types: &[GraphqlType],
    current_type: &GraphqlType,
    selected_paths: &std::collections::HashSet<String>,
    current_path: &str,
) -> String {
    let mut variables = String::new();
    let fields = current_type
        .fields
        .as_ref()
        .or(current_type.input_fields.as_ref());

    if let Some(fields) = fields {
        for field in fields {
            let field_path = format!("{}.{}", current_path, field.name);

            // Check if this field or its children are selected
            let is_selected = selected_paths.contains(&field_path);
            let has_selected_children = selected_paths
                .iter()
                .any(|p| p.starts_with(&format!("{}.", field_path)));

            if !is_selected && !has_selected_children {
                continue;
            }

            // Check for selected arguments
            for arg in &field.args {
                let arg_path = format!("{}.arg:{}", field_path, arg.name);
                if selected_paths.contains(&arg_path) {
                    // Add variable definition
                    let var_name = arg.name.clone();
                    let var_type = arg.arg_type.to_string();
                    variables.push_str(&format!("  ${}: {}\n", var_name, var_type));
                }
            }

            // Recurse for nested fields
            let base_name = field.field_type.get_base_name();
            if let Some(sub_type) = all_types
                .iter()
                .find(|t| t.name.as_deref() == base_name.map(|s| s.as_str()))
            {
                let has_selected_children = selected_paths
                    .iter()
                    .any(|p| p.starts_with(&format!("{}.", field_path)) && !p.contains(".arg:"));

                if has_selected_children {
                    variables.push_str(&generate_variables(
                        all_types,
                        sub_type,
                        selected_paths,
                        &field_path,
                    ));
                }
            }
        }
    }
    variables
}
