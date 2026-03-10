use std::collections::HashMap;

use crate::condition::EvalContext;
use crate::config::{FileList, Group, GroupType, InstallStep, ModuleConfig, PluginType};

/// A planned file operation: copy source to destination with priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOperation {
    pub source: String,
    pub destination: String,
    pub is_folder: bool,
    pub priority: i32,
}

/// The resolved installation plan after all user selections.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// File operations sorted by priority (lower first, higher overwrites).
    pub operations: Vec<FileOperation>,
}

/// Drives the FOMOD installation process.
///
/// Tracks user selections and condition flags, then resolves the final
/// set of file operations.
pub struct Installer {
    config: ModuleConfig,
    ctx: EvalContext,
    /// Maps (step_index, group_index) -> set of selected plugin indices.
    selections: HashMap<(usize, usize), Vec<usize>>,
}

impl Installer {
    pub fn new(config: ModuleConfig) -> Self {
        Self {
            config,
            ctx: EvalContext::new(),
            selections: HashMap::new(),
        }
    }

    /// Create an installer with pre-populated game environment context.
    pub fn with_context(config: ModuleConfig, ctx: EvalContext) -> Self {
        Self {
            config,
            ctx,
            selections: HashMap::new(),
        }
    }

    pub fn context(&self) -> &EvalContext {
        &self.ctx
    }

    pub fn context_mut(&mut self) -> &mut EvalContext {
        &mut self.ctx
    }

    pub fn config(&self) -> &ModuleConfig {
        &self.config
    }

    /// Check module-level dependencies. Returns `true` if satisfied.
    pub fn check_dependencies(&self) -> bool {
        self.config
            .module_dependencies
            .as_ref()
            .map(|d| d.evaluate(&self.ctx))
            .unwrap_or(true)
    }

    /// Get visible install steps (steps whose visibility conditions are met).
    pub fn visible_steps(&self) -> Vec<(usize, &InstallStep)> {
        let steps = match self.config.install_steps {
            Some(ref s) => &s.steps,
            None => return vec![],
        };

        steps
            .iter()
            .enumerate()
            .filter(|(_, step)| {
                step.visible
                    .as_ref()
                    .map(|v| v.evaluate(&self.ctx))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Record user selections for a group within a step.
    ///
    /// `plugin_indices` are indices into the group's plugin list.
    pub fn select(
        &mut self,
        step_index: usize,
        group_index: usize,
        plugin_indices: Vec<usize>,
    ) {
        self.selections
            .insert((step_index, group_index), plugin_indices.clone());

        // Collect flag updates from group plugins, then apply them.
        // Two-phase approach avoids borrowing self.config and self.ctx simultaneously.
        let mut flags_to_clear: Vec<String> = Vec::new();
        let mut flags_to_set: Vec<(String, String)> = Vec::new();

        if let Some(group) = self.get_group(step_index, group_index) {
            // Gather all flag names from every plugin in this group (to clear)
            for plugin in &group.plugins.plugins {
                if let Some(ref flags) = plugin.condition_flags {
                    for flag in &flags.flags {
                        flags_to_clear.push(flag.name.clone());
                    }
                }
            }
            // Gather flags from selected plugins (to set)
            for &idx in &plugin_indices {
                if let Some(plugin) = group.plugins.plugins.get(idx) {
                    if let Some(ref flags) = plugin.condition_flags {
                        for flag in &flags.flags {
                            flags_to_set.push((flag.name.clone(), flag.value.clone()));
                        }
                    }
                }
            }
        }

        for name in flags_to_clear {
            self.ctx.flags.remove(&name);
        }
        for (name, value) in flags_to_set {
            self.ctx.set_flag(name, value);
        }
    }

    /// Get the default selections for a group based on plugin types.
    pub fn default_selections(group: &Group) -> Vec<usize> {
        match group.group_type {
            GroupType::SelectAll => (0..group.plugins.plugins.len()).collect(),
            GroupType::SelectExactlyOne | GroupType::SelectAtMostOne => {
                // Select first Required or Recommended plugin
                group
                    .plugins
                    .plugins
                    .iter()
                    .position(|p| {
                        matches!(
                            p.plugin_type(),
                            PluginType::Required | PluginType::Recommended
                        )
                    })
                    .map(|i| vec![i])
                    .unwrap_or_default()
            }
            GroupType::SelectAtLeastOne | GroupType::SelectAny => group
                .plugins
                .plugins
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    matches!(
                        p.plugin_type(),
                        PluginType::Required | PluginType::Recommended
                    )
                })
                .map(|(i, _)| i)
                .collect(),
        }
    }

    /// Validate selections against group type constraints.
    pub fn validate_selection(group: &Group, selected: &[usize]) -> Result<(), SelectionError> {
        let count = selected.len();
        let max = group.plugins.plugins.len();

        // Check bounds
        if selected.iter().any(|&i| i >= max) {
            return Err(SelectionError::OutOfBounds);
        }

        match group.group_type {
            GroupType::SelectExactlyOne if count != 1 => {
                Err(SelectionError::InvalidCount {
                    expected: "exactly 1",
                    got: count,
                })
            }
            GroupType::SelectAtMostOne if count > 1 => {
                Err(SelectionError::InvalidCount {
                    expected: "at most 1",
                    got: count,
                })
            }
            GroupType::SelectAtLeastOne if count < 1 => {
                Err(SelectionError::InvalidCount {
                    expected: "at least 1",
                    got: count,
                })
            }
            GroupType::SelectAll if count != max => {
                Err(SelectionError::InvalidCount {
                    expected: "all",
                    got: count,
                })
            }
            _ => Ok(()),
        }
    }

    /// Resolve the final installation plan from all selections.
    pub fn resolve(&self) -> InstallPlan {
        // Collect all operations, then sort by priority for deterministic ordering.
        // Higher priority values overwrite lower when source/destination collide.
        let mut ops: Vec<FileOperation> = Vec::new();

        let mut add_files = |files: &FileList| {
            for item in &files.items {
                let r = item.as_ref();
                ops.push(FileOperation {
                    source: r.source.clone(),
                    destination: r.destination.clone(),
                    is_folder: item.is_folder(),
                    priority: r.priority,
                });
            }
        };

        // 1. Required install files (always installed)
        if let Some(ref files) = self.config.required_install_files {
            add_files(files);
        }

        // 2. Files from selected plugins
        for (&(step_idx, group_idx), selected) in &self.selections {
            if let Some(group) = self.get_group(step_idx, group_idx) {
                for &plugin_idx in selected {
                    if let Some(plugin) = group.plugins.plugins.get(plugin_idx) {
                        if let Some(ref files) = plugin.files {
                            add_files(files);
                        }
                    }
                }
            }
        }

        // 3. Conditional file installs (patterns evaluated against final flags)
        if let Some(ref cfi) = self.config.conditional_file_installs {
            for pattern in &cfi.patterns.patterns {
                if pattern.dependencies.evaluate(&self.ctx) {
                    add_files(&pattern.files);
                }
            }
        }

        ops.sort_by_key(|op| op.priority);

        InstallPlan { operations: ops }
    }

    fn get_group(&self, step_index: usize, group_index: usize) -> Option<&Group> {
        self.config
            .install_steps
            .as_ref()?
            .steps
            .get(step_index)?
            .optional_file_groups
            .as_ref()?
            .groups
            .get(group_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    OutOfBounds,
    InvalidCount {
        expected: &'static str,
        got: usize,
    },
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionError::OutOfBounds => write!(f, "plugin index out of bounds"),
            SelectionError::InvalidCount { expected, got } => {
                write!(f, "expected {expected} selections, got {got}")
            }
        }
    }
}

impl std::error::Error for SelectionError {}
