use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::ModuleConfig;
use crate::installer::Installer;

/// Current schema version for `DeclarativeConfig`.
///
/// Bump this when the config format changes in incompatible ways.
pub const SCHEMA_VERSION: u32 = 1;

/// Declarative FOMOD installation configuration.
///
/// Instead of stepping through an interactive wizard, all selections are
/// specified upfront by name. This makes FOMOD installations reproducible
/// and compatible with declarative systems like NixOS.
///
/// Uses the nixpkgs `rev` + `hash` pattern: `rev` is a human-readable
/// version identifier and `hash` is an SRI hash of the installer XML
/// for integrity verification.
///
/// ## Structure
///
/// ```nix
/// {
///   schema_version = 1;
///   rev = "1.2.0";
///   hash = "sha256-xbenkdP6HEXuWl9K...";
///   selections = {
///     "Choose Version" = {
///       "Platform" = [ "SSE" ];
///     };
///   };
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeConfig {
    /// Schema version of this config format. See [`SCHEMA_VERSION`].
    pub schema_version: u32,

    /// Human-readable revision of the FOMOD installer (e.g. mod version).
    ///
    /// Sourced from `info.xml` when available, otherwise the module name.
    pub rev: String,

    /// SRI hash of the source FOMOD XML (`sha256-<base64>`).
    ///
    /// Follows the nixpkgs convention. Used to detect when the upstream
    /// installer has changed and this config may need to be regenerated.
    pub hash: String,

    /// Step name → group name → list of selected plugin names.
    pub selections: HashMap<String, HashMap<String, Vec<String>>>,
}

/// Error from applying a declarative config.
#[derive(Debug, Clone)]
pub enum DeclarativeError {
    /// Config schema version is newer than this library supports.
    UnsupportedVersion { config: u32, supported: u32 },
    /// The installer XML hash doesn't match. The installer has been updated.
    HashMismatch { expected: String, actual: String },
    /// Referenced step name not found in the FOMOD config.
    StepNotFound(String),
    /// Referenced group name not found within the given step.
    GroupNotFound { step: String, group: String },
    /// Referenced plugin name not found within the given group.
    PluginNotFound {
        step: String,
        group: String,
        plugin: String,
    },
    /// Selection violates the group's constraints.
    ValidationFailed {
        step: String,
        group: String,
        message: String,
    },
}

impl std::fmt::Display for DeclarativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarativeError::UnsupportedVersion { config, supported } => write!(
                f,
                "config schema version {config} is not supported (this library supports up to {supported})"
            ),
            DeclarativeError::HashMismatch { expected, actual } => write!(
                f,
                "hash mismatch: config expects {expected}, got {actual}"
            ),
            DeclarativeError::StepNotFound(name) => {
                write!(f, "step not found: {name:?}")
            }
            DeclarativeError::GroupNotFound { step, group } => {
                write!(f, "group {group:?} not found in step {step:?}")
            }
            DeclarativeError::PluginNotFound {
                step,
                group,
                plugin,
            } => write!(
                f,
                "plugin {plugin:?} not found in group {group:?} (step {step:?})"
            ),
            DeclarativeError::ValidationFailed {
                step,
                group,
                message,
            } => write!(
                f,
                "invalid selection for group {group:?} in step {step:?}: {message}"
            ),
        }
    }
}

impl std::error::Error for DeclarativeError {}

/// Compute the SRI hash of FOMOD XML content, returned as `sha256-<base64>`.
///
/// Follows the [Subresource Integrity](https://www.w3.org/TR/SRI/) format
/// used by nixpkgs for content-addressed integrity verification.
pub fn hash_xml(xml: &str) -> String {
    let digest = Sha256::digest(xml.as_bytes());
    format!("sha256-{}", BASE64.encode(digest))
}

impl DeclarativeConfig {
    /// Create a template config from a FOMOD module with default selections.
    ///
    /// - `xml`: raw XML source used to compute the SRI hash
    /// - `rev`: human-readable version identifier (e.g. from `info.xml`)
    /// - `config`: parsed FOMOD module configuration
    pub fn from_defaults(xml: &str, rev: impl Into<String>, config: &ModuleConfig) -> Self {
        let selections = Self::build_selections(config, false);
        Self {
            schema_version: SCHEMA_VERSION,
            rev: rev.into(),
            hash: hash_xml(xml),
            selections,
        }
    }

    /// Create a template config with all available options listed.
    ///
    /// Every plugin in every group is included, making it easy to see what's
    /// available and remove what you don't want.
    pub fn from_all(xml: &str, rev: impl Into<String>, config: &ModuleConfig) -> Self {
        let selections = Self::build_selections(config, true);
        Self {
            schema_version: SCHEMA_VERSION,
            rev: rev.into(),
            hash: hash_xml(xml),
            selections,
        }
    }

    fn build_selections(
        config: &ModuleConfig,
        include_all: bool,
    ) -> HashMap<String, HashMap<String, Vec<String>>> {
        let mut selections = HashMap::new();

        if let Some(ref install_steps) = config.install_steps {
            for step in &install_steps.steps {
                let mut group_map = HashMap::new();

                if let Some(ref groups) = step.optional_file_groups {
                    for group in &groups.groups {
                        let plugin_names = if include_all {
                            group
                                .plugins
                                .plugins
                                .iter()
                                .map(|p| p.name.clone())
                                .collect()
                        } else {
                            Installer::default_selections(group)
                                .into_iter()
                                .filter_map(|idx| {
                                    group.plugins.plugins.get(idx).map(|p| p.name.clone())
                                })
                                .collect()
                        };
                        group_map.insert(group.name.clone(), plugin_names);
                    }
                }

                selections.insert(step.name.clone(), group_map);
            }
        }

        selections
    }

    /// Apply this declarative config to an installer, resolving names to indices.
    ///
    /// `xml` is the raw FOMOD XML source, used to verify the installer hash.
    ///
    /// Steps not present in `selections` use default selections. Groups not
    /// present in a step's map also use defaults. This allows partial configs
    /// where you only override what you care about.
    pub fn apply(&self, xml: &str, installer: &mut Installer) -> Result<(), DeclarativeError> {
        // Verify schema version
        if self.schema_version > SCHEMA_VERSION {
            return Err(DeclarativeError::UnsupportedVersion {
                config: self.schema_version,
                supported: SCHEMA_VERSION,
            });
        }

        // Verify SRI hash
        let actual_hash = hash_xml(xml);
        if self.hash != actual_hash {
            return Err(DeclarativeError::HashMismatch {
                expected: self.hash.clone(),
                actual: actual_hash,
            });
        }

        // Phase 1: resolve all names to indices and validate (immutable borrow only)
        let resolved = self.resolve_selections(installer.config())?;

        // Phase 2: apply resolved selections (mutable borrow)
        for (step_idx, group_idx, plugin_indices) in resolved {
            installer.select(step_idx, group_idx, plugin_indices);
        }

        Ok(())
    }

    /// Resolve declarative name-based selections to index-based selections.
    ///
    /// Returns a list of `(step_idx, group_idx, plugin_indices)` tuples ready
    /// to be passed to [`Installer::select`].
    fn resolve_selections(
        &self,
        config: &ModuleConfig,
    ) -> Result<Vec<(usize, usize, Vec<usize>)>, DeclarativeError> {
        let steps = match config.install_steps {
            Some(ref s) => &s.steps,
            None => return Ok(vec![]),
        };

        // Validate that all referenced names exist
        for (step_name, groups) in &self.selections {
            let step = steps
                .iter()
                .find(|s| s.name == *step_name)
                .ok_or_else(|| DeclarativeError::StepNotFound(step_name.clone()))?;

            for (group_name, plugins) in groups {
                let group = step
                    .optional_file_groups
                    .as_ref()
                    .and_then(|gl| gl.groups.iter().find(|g| g.name == *group_name))
                    .ok_or_else(|| DeclarativeError::GroupNotFound {
                        step: step_name.clone(),
                        group: group_name.clone(),
                    })?;

                for plugin_name in plugins {
                    if !group.plugins.plugins.iter().any(|p| p.name == *plugin_name) {
                        return Err(DeclarativeError::PluginNotFound {
                            step: step_name.clone(),
                            group: group_name.clone(),
                            plugin: plugin_name.clone(),
                        });
                    }
                }
            }
        }

        // Resolve names to indices
        let mut resolved = Vec::new();

        for (step_idx, step) in steps.iter().enumerate() {
            let step_selections = self.selections.get(&step.name);

            if let Some(ref groups) = step.optional_file_groups {
                for (group_idx, group) in groups.groups.iter().enumerate() {
                    let plugin_indices = match step_selections.and_then(|s| s.get(&group.name)) {
                        Some(plugin_names) => {
                            let indices: Vec<usize> = plugin_names
                                .iter()
                                .map(|name| {
                                    group
                                        .plugins
                                        .plugins
                                        .iter()
                                        .position(|p| p.name == *name)
                                        .unwrap() // Already validated above
                                })
                                .collect();

                            Installer::validate_selection(group, &indices).map_err(|e| {
                                DeclarativeError::ValidationFailed {
                                    step: step.name.clone(),
                                    group: group.name.clone(),
                                    message: e.to_string(),
                                }
                            })?;

                            indices
                        }
                        None => Installer::default_selections(group),
                    };

                    resolved.push((step_idx, group_idx, plugin_indices));
                }
            }
        }

        Ok(resolved)
    }

    /// Load a declarative config from a JSON string.
    #[cfg(feature = "json")]
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Serialize this config to a JSON string.
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load a declarative config from a RON string.
    #[cfg(feature = "ron")]
    pub fn from_ron(s: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(s)
    }

    /// Serialize this config to a RON string.
    #[cfg(feature = "ron")]
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }

    /// Serialize this config to a Nix expression string via ronix.
    #[cfg(feature = "nix")]
    pub fn to_nix(&self) -> Result<String, ronix::Error> {
        ronix::to_nix(self)
    }

    /// Serialize this config as a NixOS module fragment under the given attribute path.
    #[cfg(feature = "nix")]
    pub fn to_nix_module(&self, attr_path: &str) -> Result<String, ronix::Error> {
        ronix::to_nix_module(self, attr_path)
    }

    /// Generate a human-readable summary of all selections.
    pub fn summary(&self) -> Vec<SelectionSummary> {
        let mut result = Vec::new();
        for (step_name, groups) in &self.selections {
            for (group_name, plugins) in groups {
                result.push(SelectionSummary {
                    step: step_name.clone(),
                    group: group_name.clone(),
                    plugins: plugins.clone(),
                });
            }
        }
        result.sort_by(|a, b| (&a.step, &a.group).cmp(&(&b.step, &b.group)));
        result
    }

    /// Compute the differences between this config and another.
    pub fn diff(&self, other: &DeclarativeConfig) -> Vec<SelectionDiff> {
        let mut diffs = Vec::new();

        // Collect all step+group keys
        let mut all_keys: Vec<(String, String)> = Vec::new();
        for (step, groups) in &self.selections {
            for group in groups.keys() {
                all_keys.push((step.clone(), group.clone()));
            }
        }
        for (step, groups) in &other.selections {
            for group in groups.keys() {
                let key = (step.clone(), group.clone());
                if !all_keys.contains(&key) {
                    all_keys.push(key);
                }
            }
        }
        all_keys.sort();

        for (step, group) in all_keys {
            let self_plugins = self
                .selections
                .get(&step)
                .and_then(|g| g.get(&group))
                .cloned()
                .unwrap_or_default();
            let other_plugins = other
                .selections
                .get(&step)
                .and_then(|g| g.get(&group))
                .cloned()
                .unwrap_or_default();

            if self_plugins != other_plugins {
                diffs.push(SelectionDiff {
                    step,
                    group,
                    left: self_plugins,
                    right: other_plugins,
                });
            }
        }

        diffs
    }
}

/// A single step/group selection entry for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSummary {
    pub step: String,
    pub group: String,
    pub plugins: Vec<String>,
}

impl std::fmt::Display for SelectionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} > {}: [{}]",
            self.step,
            self.group,
            self.plugins.join(", ")
        )
    }
}

/// A difference between two declarative configs for a specific group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionDiff {
    pub step: String,
    pub group: String,
    pub left: Vec<String>,
    pub right: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_XML: &str = r#"
        <config>
            <moduleName>Test</moduleName>
            <installSteps order="Explicit">
                <installStep name="Step1">
                    <optionalFileGroups>
                        <group name="Group1" type="SelectExactlyOne">
                            <plugins>
                                <plugin name="PluginA">
                                    <typeDescriptor><type name="Recommended"/></typeDescriptor>
                                    <files><file source="a.esp" destination="Data"/></files>
                                </plugin>
                                <plugin name="PluginB">
                                    <typeDescriptor><type name="Optional"/></typeDescriptor>
                                    <files><file source="b.esp" destination="Data"/></files>
                                </plugin>
                            </plugins>
                        </group>
                    </optionalFileGroups>
                </installStep>
            </installSteps>
        </config>
    "#;

    // ---- hash_xml ----

    #[test]
    fn hash_xml_starts_with_sha256() {
        let h = hash_xml("hello");
        assert!(h.starts_with("sha256-"));
    }

    #[test]
    fn hash_xml_deterministic() {
        assert_eq!(hash_xml("test"), hash_xml("test"));
    }

    #[test]
    fn hash_xml_different_inputs() {
        assert_ne!(hash_xml("a"), hash_xml("b"));
    }

    #[test]
    fn hash_xml_empty_string() {
        let h = hash_xml("");
        assert!(h.starts_with("sha256-"));
        assert!(h.len() > 7); // sha256- + base64
    }

    #[test]
    fn hash_xml_whitespace_matters() {
        assert_ne!(hash_xml("<config/>"), hash_xml("<config />"));
    }

    // ---- from_defaults ----

    #[test]
    fn from_defaults_has_schema_version() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let decl = DeclarativeConfig::from_defaults(SIMPLE_XML, "1.0", &config);
        assert_eq!(decl.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn from_defaults_has_correct_hash() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let decl = DeclarativeConfig::from_defaults(SIMPLE_XML, "1.0", &config);
        assert_eq!(decl.hash, hash_xml(SIMPLE_XML));
    }

    #[test]
    fn from_defaults_selects_recommended() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let decl = DeclarativeConfig::from_defaults(SIMPLE_XML, "1.0", &config);
        let plugins = &decl.selections["Step1"]["Group1"];
        assert_eq!(plugins, &vec!["PluginA".to_string()]);
    }

    #[test]
    fn from_defaults_rev_stored() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let decl = DeclarativeConfig::from_defaults(SIMPLE_XML, "myrev", &config);
        assert_eq!(decl.rev, "myrev");
    }

    // ---- from_all ----

    #[test]
    fn from_all_includes_every_plugin() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let decl = DeclarativeConfig::from_all(SIMPLE_XML, "1.0", &config);
        let plugins = &decl.selections["Step1"]["Group1"];
        assert_eq!(
            plugins,
            &vec!["PluginA".to_string(), "PluginB".to_string()]
        );
    }

    // ---- apply ----

    #[test]
    fn apply_rejects_future_version() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION + 1,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::new(),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(
            err,
            DeclarativeError::UnsupportedVersion { .. }
        ));
    }

    #[test]
    fn apply_allows_current_version() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::from([(
                "Step1".into(),
                HashMap::from([("Group1".into(), vec!["PluginA".into()])]),
            )]),
        };
        assert!(decl.apply(SIMPLE_XML, &mut installer).is_ok());
    }

    #[test]
    fn apply_rejects_hash_mismatch() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: "sha256-WRONG".into(),
            selections: HashMap::new(),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(err, DeclarativeError::HashMismatch { .. }));
    }

    #[test]
    fn apply_step_not_found() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::from([("NoSuchStep".into(), HashMap::new())]),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(err, DeclarativeError::StepNotFound(_)));
    }

    #[test]
    fn apply_group_not_found() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::from([(
                "Step1".into(),
                HashMap::from([("NoSuchGroup".into(), vec![])]),
            )]),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(err, DeclarativeError::GroupNotFound { .. }));
    }

    #[test]
    fn apply_plugin_not_found() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::from([(
                "Step1".into(),
                HashMap::from([("Group1".into(), vec!["NoSuchPlugin".into()])]),
            )]),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(err, DeclarativeError::PluginNotFound { .. }));
    }

    #[test]
    fn apply_validation_fails_too_many() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::from([(
                "Step1".into(),
                HashMap::from([(
                    "Group1".into(),
                    vec!["PluginA".into(), "PluginB".into()],
                )]),
            )]),
        };
        let err = decl.apply(SIMPLE_XML, &mut installer).unwrap_err();
        assert!(matches!(err, DeclarativeError::ValidationFailed { .. }));
    }

    #[test]
    fn apply_empty_selections_uses_defaults() {
        let config = ModuleConfig::parse(SIMPLE_XML).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(SIMPLE_XML),
            selections: HashMap::new(), // empty → defaults
        };
        decl.apply(SIMPLE_XML, &mut installer).unwrap();
        let plan = installer.resolve();
        // Default is PluginA (Recommended)
        assert!(plan.operations.iter().any(|op| op.source == "a.esp"));
    }

    #[test]
    fn apply_no_install_steps() {
        let xml = r#"<config><moduleName>T</moduleName></config>"#;
        let config = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config);
        let decl = DeclarativeConfig {
            schema_version: SCHEMA_VERSION,
            rev: "".into(),
            hash: hash_xml(xml),
            selections: HashMap::new(),
        };
        assert!(decl.apply(xml, &mut installer).is_ok());
    }

    // ---- DeclarativeError Display ----

    #[test]
    fn error_display_unsupported_version() {
        let err = DeclarativeError::UnsupportedVersion {
            config: 99,
            supported: 1,
        };
        let s = err.to_string();
        assert!(s.contains("99"));
        assert!(s.contains("1"));
    }

    #[test]
    fn error_display_hash_mismatch() {
        let err = DeclarativeError::HashMismatch {
            expected: "sha256-AAA".into(),
            actual: "sha256-BBB".into(),
        };
        let s = err.to_string();
        assert!(s.contains("sha256-AAA"));
        assert!(s.contains("sha256-BBB"));
    }

    #[test]
    fn error_display_step_not_found() {
        let err = DeclarativeError::StepNotFound("MyStep".into());
        assert!(err.to_string().contains("MyStep"));
    }

    #[test]
    fn error_display_group_not_found() {
        let err = DeclarativeError::GroupNotFound {
            step: "S".into(),
            group: "G".into(),
        };
        let s = err.to_string();
        assert!(s.contains("S"));
        assert!(s.contains("G"));
    }

    #[test]
    fn error_display_plugin_not_found() {
        let err = DeclarativeError::PluginNotFound {
            step: "S".into(),
            group: "G".into(),
            plugin: "P".into(),
        };
        let s = err.to_string();
        assert!(s.contains("P"));
    }

    #[test]
    fn error_display_validation_failed() {
        let err = DeclarativeError::ValidationFailed {
            step: "S".into(),
            group: "G".into(),
            message: "too many".into(),
        };
        assert!(err.to_string().contains("too many"));
    }
}
