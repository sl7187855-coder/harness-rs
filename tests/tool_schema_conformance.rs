use harness_rs::tool_catalog;
use serde_json::{Value, json};

#[test]
fn checked_in_fixture_matches_all_tool_schemas_exactly() {
    let actual: Vec<Value> = tool_catalog::all()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "group": spec.group,
                "state": spec.state,
                "function": spec.openai()["function"].clone(),
            })
        })
        .collect();
    if std::env::var_os("UPDATE_TOOL_SCHEMA_FIXTURE").is_some() {
        let serialized = serde_json::to_string_pretty(&actual).expect("tool schemas serialize");
        std::fs::write(
            "tests/fixtures/tool_schemas.json",
            format!("{serialized}\n"),
        )
        .expect("write reviewed tool schema fixture");
        return;
    }
    let fixture: Vec<Value> = serde_json::from_str(include_str!("fixtures/tool_schemas.json"))
        .expect("tool schema fixture must be valid JSON");
    let actual_by_name = actual
        .iter()
        .map(|item| (item["name"].as_str().expect("tool name is a string"), item))
        .collect::<std::collections::BTreeMap<_, _>>();
    let fixture_by_name = fixture
        .iter()
        .map(|item| {
            (
                item["name"]
                    .as_str()
                    .expect("fixture tool name is a string"),
                item,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        actual_by_name.keys().collect::<Vec<_>>(),
        fixture_by_name.keys().collect::<Vec<_>>(),
        "catalogue and fixture must have identical tool names"
    );
    for (name, actual_schema) in actual_by_name {
        let expected = fixture_by_name[name];
        assert_eq!(
            actual_schema, expected,
            "schema mismatch for {name}; review and update the fixture intentionally"
        );
    }
}

#[test]
fn every_declared_tool_has_schema_and_unique_name() {
    let all = tool_catalog::all();
    let mut names: Vec<_> = all.iter().map(|tool| tool.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), all.len());
    for spec in all {
        let function = spec.openai()["function"].clone();
        assert_eq!(function["name"], spec.name);
        assert_eq!(function["parameters"]["type"], "object");
        assert!(function["parameters"]["properties"].is_object());
        assert!(function["parameters"]["required"].is_array());
    }
}

#[test]
fn ready_memory_tools_and_apply_patch_are_in_schema() {
    for name in ["MemorySearch", "MemorySave", "MemoryForget", "ApplyPatch"] {
        let spec = tool_catalog::find(name).unwrap_or_else(|| panic!("missing tool {name}"));
        assert!(spec.ready(), "{name} must be marked ready");
        let params = &spec.openai()["function"]["parameters"];
        assert!(params["properties"].is_object());
    }
}
