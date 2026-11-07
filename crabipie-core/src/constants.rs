
pub const BODY_DEFAULT: &str = r#"{
  "title": "foo",
  "body": "bar",
  "userId": 1,
  "foo": "bar"
}"#;

pub const RAW_FORM_PLACEHOLDER: &str = r#"Rows are separated by newline.
Keys and values are separated by :
Prepend # to the rows that you want to add but keep it disabled.
"#;

pub const INTROSPECTION_QUERY: &str = r#"
{
  __schema {
    queryType { name }
    types {
      name
      kind
      fields(includeDeprecated: false) {
        name
        type {
          name
          kind
          ofType { name kind ofType { name kind } }
        }
        args {
          name
          type { name kind ofType { name kind } }
        }
      }
      inputFields {
        name
        type {
          name
          kind
          ofType { name kind ofType { name kind } }
        }
      }
    }
  }
}
"#;
