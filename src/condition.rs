use serde::Deserialize;
use std::collections::HashMap;

/// Composite dependency with AND/OR logic, potentially nested.
#[derive(Debug, Clone, Deserialize)]
pub struct CompositeDependency {
    #[serde(rename = "@operator", default = "default_operator")]
    pub operator: Operator,

    #[serde(rename = "fileDependency", default)]
    pub file_deps: Vec<FileDependency>,

    #[serde(rename = "flagDependency", default)]
    pub flag_deps: Vec<FlagDependency>,

    #[serde(rename = "gameDependency", default)]
    pub game_deps: Vec<GameDependency>,

    /// Nested composite dependencies for complex logic.
    #[serde(rename = "dependencies", default)]
    pub nested: Vec<CompositeDependency>,
}

impl CompositeDependency {
    /// Evaluate this dependency tree against the current environment.
    pub fn evaluate(&self, ctx: &EvalContext) -> bool {
        let results = self
            .file_deps
            .iter()
            .map(|d| d.evaluate(ctx))
            .chain(self.flag_deps.iter().map(|d| d.evaluate(ctx)))
            .chain(self.game_deps.iter().map(|d| d.evaluate(ctx)))
            .chain(self.nested.iter().map(|d| d.evaluate(ctx)));

        match self.operator {
            Operator::And => results.fold(true, |acc, v| acc && v),
            Operator::Or => results.fold(false, |acc, v| acc || v),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDependency {
    #[serde(rename = "@file")]
    pub file: String,

    #[serde(rename = "@state")]
    pub state: FileState,
}

impl FileDependency {
    fn evaluate(&self, ctx: &EvalContext) -> bool {
        let actual = ctx
            .file_states
            .get(&self.file)
            .copied()
            .unwrap_or(FileState::Missing);
        actual == self.state
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlagDependency {
    #[serde(rename = "@flag")]
    pub flag: String,

    #[serde(rename = "@value")]
    pub value: String,
}

impl FlagDependency {
    fn evaluate(&self, ctx: &EvalContext) -> bool {
        ctx.flags.get(&self.flag).map(|v| v == &self.value).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameDependency {
    #[serde(rename = "@version")]
    pub version: String,
}

impl GameDependency {
    fn evaluate(&self, ctx: &EvalContext) -> bool {
        match (&ctx.game_version, &self.version) {
            (Some(current), required) => compare_versions(current, required),
            (None, _) => false,
        }
    }
}

/// Simple version comparison: current >= required.
fn compare_versions(current: &str, required: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.').filter_map(|p| p.parse().ok()).collect()
    };
    let cur = parse(current);
    let req = parse(required);
    cur >= req
}

/// Runtime context for evaluating conditions.
#[derive(Debug, Default, Clone)]
pub struct EvalContext {
    /// Flags set by user plugin selections.
    pub flags: HashMap<String, String>,

    /// Known file states in the game directory.
    pub file_states: HashMap<String, FileState>,

    /// Current game version (e.g. "1.5.0.0").
    pub game_version: Option<String>,
}

impl EvalContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_flag(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.flags.insert(name.into(), value.into());
    }

    pub fn set_file_state(&mut self, file: impl Into<String>, state: FileState) {
        self.file_states.insert(file.into(), state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum FileState {
    Active,
    Inactive,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Operator {
    And,
    Or,
}

fn default_operator() -> Operator {
    Operator::And
}
