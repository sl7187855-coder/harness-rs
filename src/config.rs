use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub workspace: PathBuf,
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub skills_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub hud_file: PathBuf,
    pub api_base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tool_output: usize,
    pub max_steps: usize,
    pub max_parallel_tools: usize,
}
impl Config {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HARNESS_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/harness-rs"));
        let workspace = std::env::var_os("HARNESS_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("workspace"));
        let data_dir = std::env::var_os("HARNESS_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("data"));
        Self {
            bind: std::env::var("HARNESS_BIND").unwrap_or_else(|_| "127.0.0.1:1347".into()),
            database: data_dir.join("harness.db"),
            skills_dir: std::env::var_os("HARNESS_SKILLS")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("skills")),
            plugins_dir: std::env::var_os("HARNESS_PLUGINS")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("plugins")),
            hud_file: std::env::var_os("HARNESS_HUD_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("web/thomas-hud.html")),
            api_base_url: std::env::var("HARNESS_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com/v1".into()),
            api_key: std::env::var("HARNESS_API_KEY").unwrap_or_default(),
            model: std::env::var("HARNESS_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
            max_tool_output: std::env::var("HARNESS_MAX_TOOL_OUTPUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24_000),
            max_steps: std::env::var("HARNESS_MAX_STEPS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(40),
            max_parallel_tools: std::env::var("HARNESS_MAX_PARALLEL_TOOLS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            workspace,
            data_dir,
        }
    }
}
