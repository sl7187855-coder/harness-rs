use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    pub state: &'static str,
}
impl ToolSpec {
    pub fn ready(&self) -> bool {
        self.state == "ready"
    }
    pub fn openai(&self) -> Value {
        let (description, parameters) = schema(self.name, self.description);
        json!({"type":"function","function":{"name":self.name,"description":description,"parameters":parameters}})
    }
}

const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "Read",
        group: "files",
        description: "Read a file as numbered lines; supports offset and limit. Never read a directory.",
        state: "ready",
    },
    ToolSpec {
        name: "Write",
        group: "files",
        description: "Create or atomically replace a file. Existing files must be read in this session first.",
        state: "ready",
    },
    ToolSpec {
        name: "Edit",
        group: "files",
        description: "Replace an exact unique text fragment. Requires a prior Read of the file and exactly one match unless replace_all is true.",
        state: "ready",
    },
    ToolSpec {
        name: "NotebookEdit",
        group: "files",
        description: "Edit a Jupyter notebook cell by id or index; replacing a cell clears stale outputs and execution count.",
        state: "ready",
    },
    ToolSpec {
        name: "Glob",
        group: "files",
        description: "Find paths by glob pattern, bounded and sorted; optional ignore support.",
        state: "ready",
    },
    ToolSpec {
        name: "Grep",
        group: "files",
        description: "Search file contents with regex and bounded results; ignores binary files and respects .gitignore by default.",
        state: "ready",
    },
    ToolSpec {
        name: "Bash",
        group: "execution",
        description: "Run one shell command in the project workspace with timeout, bounded output and explicit permission policy.",
        state: "ready",
    },
    ToolSpec {
        name: "PowerShell",
        group: "execution",
        description: "Run PowerShell commands on Windows hosts. This Linux host reports unsupported.",
        state: "deferred",
    },
    ToolSpec {
        name: "Monitor",
        group: "execution",
        description: "Poll a background task by task_id and read bounded output from its log.",
        state: "ready",
    },
    ToolSpec {
        name: "Agent",
        group: "agents",
        description: "Spawn a bounded background subagent with independent context and the intersection of parent tool permissions.",
        state: "ready",
    },
    ToolSpec {
        name: "SendMessage",
        group: "agents",
        description: "Send a steering message to a running subagent by agent id; delivered before its next model step.",
        state: "ready",
    },
    ToolSpec {
        name: "ListAgents",
        group: "agents",
        description: "List subagents owned by the current session, with status, allowed tools and result summary.",
        state: "ready",
    },
    ToolSpec {
        name: "SubagentHandback",
        group: "agents",
        description: "Return a structured report from a subagent run to the parent session and end the child turn.",
        state: "ready",
    },
    ToolSpec {
        name: "Workflow",
        group: "agents",
        description: "Launch a bounded parallel workflow of one to six independent agent steps; each step inherits only parent permissions.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskCreate",
        group: "tasks",
        description: "Create a task in this session task list.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskGet",
        group: "tasks",
        description: "Get details for a session task.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskList",
        group: "tasks",
        description: "List session tasks and status.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskUpdate",
        group: "tasks",
        description: "Update task status, description or dependencies.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskOutput",
        group: "tasks",
        description: "Read the result and status of a background agent task.",
        state: "ready",
    },
    ToolSpec {
        name: "TaskStop",
        group: "tasks",
        description: "Cancel a running background agent task by ID.",
        state: "ready",
    },
    ToolSpec {
        name: "TodoWrite",
        group: "tasks",
        description: "Replace the session checklist with an ordered set of items.",
        state: "ready",
    },
    ToolSpec {
        name: "EnterPlanMode",
        group: "planning",
        description: "Switch the session to read-only planning mode.",
        state: "ready",
    },
    ToolSpec {
        name: "ExitPlanMode",
        group: "planning",
        description: "Submit the proposed plan for approval and leave planning mode.",
        state: "ready",
    },
    ToolSpec {
        name: "EnterWorktree",
        group: "planning",
        description: "Create an isolated Git worktree and scope subsequent file/shell operations in this session to it.",
        state: "ready",
    },
    ToolSpec {
        name: "ExitWorktree",
        group: "planning",
        description: "Return this session to the original workspace without deleting the preserved worktree.",
        state: "ready",
    },
    ToolSpec {
        name: "CronCreate",
        group: "schedule",
        description: "Create an approved one-shot or recurring scheduled prompt. Scheduled runs have read-only tool access and results are retained in the schedule list.",
        state: "ready",
    },
    ToolSpec {
        name: "CronDelete",
        group: "schedule",
        description: "Delete one of the current session's scheduled jobs after approval.",
        state: "ready",
    },
    ToolSpec {
        name: "CronList",
        group: "schedule",
        description: "List schedules for the current session, including next run time and last execution result.",
        state: "ready",
    },
    ToolSpec {
        name: "ScheduleWakeup",
        group: "schedule",
        description: "Set or clear one read-only autonomous wakeup iteration; there is at most one pending wakeup per session.",
        state: "ready",
    },
    ToolSpec {
        name: "RemoteTrigger",
        group: "schedule",
        description: "Trigger a registered remote routine through a configured connector.",
        state: "deferred",
    },
    ToolSpec {
        name: "WebFetch",
        group: "web",
        description: "Fetch a public HTTPS URL with DNS/IP checks, redirect blocking and a bounded response size.",
        state: "ready",
    },
    ToolSpec {
        name: "WebSearch",
        group: "web",
        description: "Search public web results and return bounded titles, URLs and snippets.",
        state: "ready",
    },
    ToolSpec {
        name: "Skill",
        group: "extensions",
        description: "Load a local SKILL.md and apply its instructions within current permissions.",
        state: "ready",
    },
    ToolSpec {
        name: "ToolSearch",
        group: "extensions",
        description: "Search the full tool catalogue and load matching ready tools into the active tool list.",
        state: "ready",
    },
    ToolSpec {
        name: "ListMcpResourcesTool",
        group: "extensions",
        description: "List resources exposed by explicitly configured MCP stdio servers running in the read-only sandbox.",
        state: "ready",
    },
    ToolSpec {
        name: "ReadMcpResourceTool",
        group: "extensions",
        description: "Read a resource URI from an explicitly configured MCP stdio server in the read-only sandbox.",
        state: "ready",
    },
    ToolSpec {
        name: "WaitForMcpServers",
        group: "extensions",
        description: "Probe and initialize explicitly configured MCP stdio servers with a bounded startup timeout; each probe is short-lived.",
        state: "ready",
    },
    ToolSpec {
        name: "LSP",
        group: "code",
        description: "Use an explicitly configured sandboxed language server for definition, references and diagnostics in workspace files.",
        state: "ready",
    },
    ToolSpec {
        name: "AskUserQuestion",
        group: "interaction",
        description: "Ask the user a structured multi-choice question and wait for an answer.",
        state: "ready",
    },
    ToolSpec {
        name: "PushNotification",
        group: "interaction",
        description: "Send a short notification through the active HUD; browser notifications honor the user's preference and permission.",
        state: "ready",
    },
    ToolSpec {
        name: "SendUserFile",
        group: "interaction",
        description: "Offer an existing file inside the workspace to the HUD as a deliverable; blocks traversal and files over 25 MiB.",
        state: "ready",
    },
    ToolSpec {
        name: "Artifact",
        group: "interaction",
        description: "Save bounded text/HTML/JSON artifacts atomically inside the project and surface a deliverable event to the HUD.",
        state: "ready",
    },
    ToolSpec {
        name: "ReportFindings",
        group: "interaction",
        description: "Validate and return bounded structured code-review findings with workspace-relative file paths.",
        state: "ready",
    },
    ToolSpec {
        name: "SendFeedback",
        group: "interaction",
        description: "Create a downloadable feedback draft in the HUD for explicit user review; it is not submitted externally.",
        state: "ready",
    },
    ToolSpec {
        name: "ShareOnboardingGuide",
        group: "interaction",
        description: "Generate a downloadable onboarding guide surfaced in the HUD; external sharing needs a configured collaboration provider.",
        state: "ready",
    },
    ToolSpec {
        name: "EndConversation",
        group: "interaction",
        description: "End the current session when the user requests it.",
        state: "ready",
    },
    ToolSpec {
        name: "MemorySearch",
        group: "memory",
        description: "Search persistent long-term memory within the requested scope; returns bounded matches.",
        state: "ready",
    },
    ToolSpec {
        name: "MemorySave",
        group: "memory",
        description: "Save a fact the user explicitly stated or confirmed; rejects obvious secrets and sensitive credentials.",
        state: "ready",
    },
    ToolSpec {
        name: "MemoryForget",
        group: "memory",
        description: "Permanently delete a saved memory by numeric id and update the full-text index.",
        state: "ready",
    },
    ToolSpec {
        name: "ApplyPatch",
        group: "files",
        description: "Apply a unified diff to files inside the workspace after reading the target files in this session.",
        state: "ready",
    },
];

pub fn all() -> &'static [ToolSpec] {
    TOOLS
}
pub fn find(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|t| t.name == name)
}
pub fn ready_names() -> Vec<&'static str> {
    // Keep the stable prompt prefix small. Other implemented tools are loaded
    // per-session through ToolSearch, while deferred tools remain discoverable.
    const INITIAL: &[&str] = &[
        "Read",
        "Write",
        "Edit",
        "NotebookEdit",
        "Glob",
        "Grep",
        "Bash",
        "Agent",
        "TaskCreate",
        "TaskGet",
        "TaskList",
        "TaskUpdate",
        "TodoWrite",
        "Skill",
        "ToolSearch",
    ];
    INITIAL
        .iter()
        .copied()
        .filter(|name| TOOLS.iter().any(|tool| tool.name == *name && tool.ready()))
        .collect()
}

pub fn openai_tools(active: &[String]) -> Vec<Value> {
    TOOLS
        .iter()
        .filter(|tool| tool.ready() && active.iter().any(|name| name == tool.name))
        .map(ToolSpec::openai)
        .collect()
}

fn schema(name: &str, description: &str) -> (String, Value) {
    let (props, required) = match name {
        "Read" => (
            json!({"path":{"type":"string","description":"Project-relative path or absolute path inside workspace"},"offset":{"type":"integer","minimum":1,"default":1},"limit":{"type":"integer","minimum":1,"maximum":2000,"default":300}}),
            vec!["path"],
        ),
        "Write" => (
            json!({"path":{"type":"string"},"content":{"type":"string"},"create_dirs":{"type":"boolean","default":false}}),
            vec!["path", "content"],
        ),
        "NotebookEdit" => (
            json!({"notebook_path":{"type":"string"},"cell_id":{"type":"string"},"cell_index":{"type":"integer","minimum":0},"new_source":{"type":"string"},"cell_type":{"type":"string","enum":["code","markdown","raw"],"default":"code"},"edit_mode":{"type":"string","enum":["replace","insert","delete"],"default":"replace"}}),
            vec!["notebook_path"],
        ),
        "Edit" => (
            json!({"path":{"type":"string"},"old_string":{"type":"string","minLength":1},"new_string":{"type":"string"},"replace_all":{"type":"boolean","default":false}}),
            vec!["path", "old_string", "new_string"],
        ),
        "Glob" => (
            json!({"pattern":{"type":"string"},"path":{"type":"string","default":"."},"respect_ignore":{"type":"boolean","default":false},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}}),
            vec!["pattern"],
        ),
        "Grep" => (
            json!({"pattern":{"type":"string"},"path":{"type":"string","default":"."},"glob":{"type":"string"},"mode":{"type":"string","enum":["files_with_matches","content","count"],"default":"files_with_matches"},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100},"case_sensitive":{"type":"boolean","default":true}}),
            vec!["pattern"],
        ),
        "Bash" => (
            json!({"command":{"type":"string"},"timeout_ms":{"type":"integer","minimum":100,"maximum":600000,"default":120000},"run_in_background":{"type":"boolean","default":false}}),
            vec!["command"],
        ),
        "Monitor" => (
            json!({"task_id":{"type":"string"},"offset":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":24000,"default":12000}}),
            vec!["task_id"],
        ),
        "TaskCreate" => (
            json!({"subject":{"type":"string"},"description":{"type":"string","default":""},"activeForm":{"type":"string","default":""},"blockedBy":{"type":"array","items":{"type":"string"},"default":[]}}),
            vec!["subject"],
        ),
        "TaskGet" => (json!({"task_id":{"type":"string"}}), vec!["task_id"]),
        "TaskList" => (json!({}), vec![]),
        "TaskUpdate" => (
            json!({"task_id":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed","deleted"]},"subject":{"type":"string"},"description":{"type":"string"},"add_blocked_by":{"type":"array","items":{"type":"string"}},"add_blocks":{"type":"array","items":{"type":"string"}}}),
            vec!["task_id"],
        ),
        "TodoWrite" => (
            json!({"todos":{"type":"array","items":{"type":"object","properties":{"content":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed"]},"activeForm":{"type":"string"}},"required":["content","status"]}}}),
            vec!["todos"],
        ),
        "EnterPlanMode" | "ExitPlanMode" | "ExitWorktree" | "EndConversation" | "ListAgents"
        | "CronList" | "WaitForMcpServers" => (json!({}), vec![]),
        "ListMcpResourcesTool" => (
            json!({"server":{"type":"string","maxLength":96,"description":"Optional configured server name"},"limit":{"type":"integer","minimum":1,"maximum":50,"default":20}}),
            vec![],
        ),
        "ReadMcpResourceTool" => (
            json!({"server":{"type":"string","minLength":1,"maxLength":96},"uri":{"type":"string","minLength":1,"maxLength":2000}}),
            vec!["server", "uri"],
        ),
        "CronCreate" => (
            json!({"name":{"type":"string","maxLength":120,"default":"Scheduled task"},"prompt":{"type":"string","minLength":1,"maxLength":8000},"delay_seconds":{"type":"integer","minimum":60,"maximum":2592000,"default":60},"interval_seconds":{"type":"integer","minimum":0,"maximum":2592000,"default":0}}),
            vec!["prompt"],
        ),
        "CronDelete" => (json!({"job_id":{"type":"string"}}), vec!["job_id"]),
        "ScheduleWakeup" => (
            json!({"action":{"type":"string","enum":["set","clear"],"default":"set"},"delay_seconds":{"type":"integer","minimum":60,"maximum":2592000,"default":3600},"prompt":{"type":"string","maxLength":8000,"default":"Do a brief read-only check of the workspace and report anything actionable."}}),
            vec!["action"],
        ),
        "EnterWorktree" => (
            json!({"branch":{"type":"string","description":"Optional new or existing branch; blank creates harness/<id>"}}),
            vec![],
        ),
        "AskUserQuestion" => (
            json!({"questions":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","properties":{"question":{"type":"string"},"header":{"type":"string"},"options":{"type":"array","minItems":2,"maxItems":4,"items":{"type":"object","properties":{"label":{"type":"string"},"description":{"type":"string"}},"required":["label","description"]}},"multiSelect":{"type":"boolean","default":false}},"required":["question","header","options","multiSelect"]}}}),
            vec!["questions"],
        ),
        "Skill" => (
            json!({"name":{"type":"string"},"args":{"type":"string","default":""}}),
            vec!["name"],
        ),
        "PushNotification" => (
            json!({"title":{"type":"string","maxLength":100,"default":"Thomas"},"message":{"type":"string","minLength":1,"maxLength":240},"severity":{"type":"string","enum":["info","success","warning","error"],"default":"info"}}),
            vec!["message"],
        ),
        "SendFeedback" => (
            json!({"subject":{"type":"string","maxLength":180,"default":"Feedback sobre o Harness"},"category":{"type":"string","enum":["bug","idea","experience","other"],"default":"other"},"feedback":{"type":"string","minLength":1,"maxLength":12000},"context":{"type":"string","maxLength":4000,"default":""}}),
            vec!["feedback"],
        ),
        "ShareOnboardingGuide" => (
            json!({"audience":{"type":"string","maxLength":120,"default":"novo usuário"},"format":{"type":"string","enum":["markdown","html"],"default":"markdown"},"include_quickstart":{"type":"boolean","default":true}}),
            vec![],
        ),
        "ToolSearch" => (
            json!({"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":20,"default":8}}),
            vec!["query"],
        ),
        "MemorySearch" => (
            json!({"query":{"type":"string","minLength":1,"maxLength":1000},"scope":{"type":"string","maxLength":160,"default":"current_project"},"limit":{"type":"integer","minimum":1,"maximum":30,"default":10}}),
            vec!["query"],
        ),
        "MemorySave" => (
            json!({"text":{"type":"string","minLength":1,"maxLength":4000},"kind":{"type":"string","maxLength":64,"default":"fact"},"scope":{"type":"string","maxLength":160,"default":"current_project"},"importance":{"type":"number","minimum":0,"maximum":1,"default":0.8},"confidence":{"type":"number","minimum":0,"maximum":1,"default":0.9}}),
            vec!["text"],
        ),
        "MemoryForget" => (
            json!({"memory_id":{"type":"integer","minimum":1}}),
            vec!["memory_id"],
        ),
        "ApplyPatch" => (
            json!({"patch":{"type":"string","minLength":1,"maxLength":100000}}),
            vec!["patch"],
        ),
        "Agent" => (
            json!({"description":{"type":"string"},"prompt":{"type":"string"},"tools":{"type":"array","items":{"type":"string"}},"maxTurns":{"type":"integer","minimum":1,"maximum":12,"default":6}}),
            vec!["description", "prompt"],
        ),
        "Workflow" => (
            json!({"workflow_name":{"type":"string"},"steps":{"type":"array","minItems":1,"maxItems":6,"items":{"type":"object","properties":{"name":{"type":"string"},"description":{"type":"string"},"prompt":{"type":"string"},"tools":{"type":"array","items":{"type":"string"}},"maxTurns":{"type":"integer","minimum":1,"maximum":10}},"required":["prompt"]}}}),
            vec!["steps"],
        ),
        "SendMessage" => (
            json!({"recipient":{"type":"string"},"message":{"type":"string","maxLength":6000}}),
            vec!["recipient", "message"],
        ),
        "Artifact" => (
            json!({"title":{"type":"string"},"name":{"type":"string"},"type":{"type":"string","enum":["markdown","html","json","text","txt"]},"content":{"type":"string","maxLength":1000000}}),
            vec!["content"],
        ),
        "SendUserFile" => (
            json!({"path":{"type":"string"},"name":{"type":"string"}}),
            vec!["path"],
        ),
        "SubagentHandback" => (
            json!({"report":{"type":"string","maxLength":12000}}),
            vec!["report"],
        ),
        "ReportFindings" => (
            json!({"findings":{"type":"array","items":{"type":"object"}}}),
            vec!["findings"],
        ),
        "WebFetch" => (
            json!({"url":{"type":"string","format":"uri"},"prompt":{"type":"string"}}),
            vec!["url", "prompt"],
        ),
        "WebSearch" => (
            json!({"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":10,"default":5}}),
            vec!["query"],
        ),
        "LSP" => (
            json!({"operation":{"type":"string","enum":["definition","references","diagnostics"]},"file":{"type":"string"},"line":{"type":"integer"},"character":{"type":"integer"}}),
            vec!["operation", "file"],
        ),
        _ => (
            json!({"input":{"type":"object","description":"Tool-specific input; refer to the tool description"}}),
            vec![],
        ),
    };
    (
        description.to_string(),
        json!({"type":"object","properties":props,"required":required,"additionalProperties":false}),
    )
}

#[derive(Debug, Deserialize)]
pub struct ToolSearchInput {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    8
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_has_50_unique_tools_with_two_deferred() {
        assert_eq!(TOOLS.len(), 50);
        let mut names: Vec<_> = TOOLS.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 50);
        for name in ["MemorySearch", "MemorySave", "MemoryForget", "ApplyPatch"] {
            assert!(
                find(name).is_some_and(ToolSpec::ready),
                "{name} must be discoverable and ready"
            );
        }
    }
    #[test]
    fn edit_schema_requires_exact_strings() {
        let spec = find("Edit").unwrap().openai();
        let schema = &spec["function"]["parameters"];
        assert_eq!(schema["required"].as_array().unwrap().len(), 3);
        assert_eq!(schema["properties"]["old_string"]["minLength"], 1);
    }
}
