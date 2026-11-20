//! GraphQL query generation from the selected schema fields.

use crate::app::CrabiPie;
use crate::domain::graphql::generate_selection_set;

impl CrabiPie {
    pub(crate) fn build_query(&self) -> String {
        let Some(tab) = self.current_tab() else {
            return String::from("Loading...");
        };

        let mut buf = String::from("query {\n");

        if let Some(schema) = &tab.graphql_schema {
            // Find the "Query" type
            let root_type_name = &schema.query_type.name;
            if let Some(root_type) = schema
                .types
                .iter()
                .find(|t| t.name.as_deref() == Some("Query"))
            {
                let selection = generate_selection_set(
                    &schema.types,
                    root_type,
                    &tab.graphql_selected_paths,
                    1, // Start at indent level 1
                    root_type_name,
                );
                buf.push_str(&selection);
            }
        }

        buf.push('}');
        buf
    }
}
