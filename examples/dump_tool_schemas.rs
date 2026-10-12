use harness_rs::tool_catalog;
use serde_json::{Value, json};

fn main() {
    let schemas: Vec<Value> = tool_catalog::all()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "group": spec.group,
                "state": spec.state,
                "function": spec.openai()["function"].clone()
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&schemas).expect("serialize schemas")
    );
}
