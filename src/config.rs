use serde::Deserialize;

use crate::condition::CompositeDependency;
use crate::error;

/// Root element of a FOMOD `ModuleConfig.xml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "config")]
pub struct ModuleConfig {
    #[serde(rename = "moduleName")]
    pub module_name: ModuleName,

    #[serde(rename = "moduleImage")]
    pub module_image: Option<ModuleImage>,

    #[serde(rename = "moduleDependencies")]
    pub module_dependencies: Option<CompositeDependency>,

    #[serde(rename = "requiredInstallFiles")]
    pub required_install_files: Option<FileList>,

    #[serde(rename = "installSteps")]
    pub install_steps: Option<InstallSteps>,

    #[serde(rename = "conditionalFileInstalls")]
    pub conditional_file_installs: Option<ConditionalFileInstalls>,
}

impl ModuleConfig {
    pub fn parse(xml: &str) -> error::Result<Self> {
        quick_xml::de::from_str(xml).map_err(Into::into)
    }
}

/// Module display name with optional positioning.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleName {
    #[serde(rename = "@position")]
    pub position: Option<NamePosition>,

    #[serde(rename = "$text")]
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum NamePosition {
    Left,
    Right,
    RightOfImage,
}

/// Header/banner image for the installer.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleImage {
    #[serde(rename = "@path")]
    pub path: String,

    #[serde(rename = "@showImage", default = "default_true")]
    pub show_image: bool,

    #[serde(rename = "@showFade", default = "default_true")]
    pub show_fade: bool,

    #[serde(rename = "@height", default = "default_neg_one")]
    pub height: i32,
}

/// Ordered sequence of installation steps (pages).
#[derive(Debug, Clone, Deserialize)]
pub struct InstallSteps {
    #[serde(rename = "@order")]
    pub order: Option<SortOrder>,

    #[serde(rename = "installStep", default)]
    pub steps: Vec<InstallStep>,
}

/// A single page presented to the user.
#[derive(Debug, Clone, Deserialize)]
pub struct InstallStep {
    #[serde(rename = "@name")]
    pub name: String,

    pub visible: Option<CompositeDependency>,

    #[serde(rename = "optionalFileGroups")]
    pub optional_file_groups: Option<GroupList>,
}

/// Container for option groups within a step.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupList {
    #[serde(rename = "@order")]
    pub order: Option<SortOrder>,

    #[serde(rename = "group", default)]
    pub groups: Vec<Group>,
}

/// A group of related installation options.
#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    #[serde(rename = "@name")]
    pub name: String,

    #[serde(rename = "@type")]
    pub group_type: GroupType,

    #[serde(rename = "plugins")]
    pub plugins: PluginList,
}

/// Container for plugins within a group.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginList {
    #[serde(rename = "@order")]
    pub order: Option<SortOrder>,

    #[serde(rename = "plugin", default)]
    pub plugins: Vec<Plugin>,
}

/// An individual installation option the user can select.
#[derive(Debug, Clone, Deserialize)]
pub struct Plugin {
    #[serde(rename = "@name")]
    pub name: String,

    pub description: Option<String>,

    pub image: Option<PluginImage>,

    #[serde(rename = "typeDescriptor")]
    pub type_descriptor: Option<TypeDescriptor>,

    #[serde(rename = "conditionFlags")]
    pub condition_flags: Option<ConditionFlagList>,

    pub files: Option<FileList>,
}

impl Plugin {
    /// Resolved plugin type, defaulting to `Optional`.
    pub fn plugin_type(&self) -> PluginType {
        self.type_descriptor
            .as_ref()
            .map(|td| td.resolved_type())
            .unwrap_or(PluginType::Optional)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginImage {
    #[serde(rename = "@path")]
    pub path: String,
}

/// Describes the selection type of a plugin. Supports both simple
/// `<type name="..."/>` and conditional `<dependencyType>` forms.
#[derive(Debug, Clone, Deserialize)]
pub struct TypeDescriptor {
    #[serde(rename = "type")]
    pub simple_type: Option<SimpleType>,

    #[serde(rename = "dependencyType")]
    pub dependency_type: Option<DependencyType>,
}

impl TypeDescriptor {
    pub fn resolved_type(&self) -> PluginType {
        if let Some(ref st) = self.simple_type {
            return st.name;
        }
        // TODO: evaluate dependency_type conditions at runtime
        if let Some(ref dt) = self.dependency_type {
            return dt.default_type.name;
        }
        PluginType::Optional
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SimpleType {
    #[serde(rename = "@name")]
    pub name: PluginType,
}

/// Conditional type descriptor — type depends on runtime conditions.
#[derive(Debug, Clone, Deserialize)]
pub struct DependencyType {
    #[serde(rename = "defaultType")]
    pub default_type: SimpleType,

    #[serde(rename = "patterns")]
    pub patterns: Option<TypePatterns>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TypePatterns {
    #[serde(rename = "pattern", default)]
    pub patterns: Vec<TypePattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TypePattern {
    pub dependencies: CompositeDependency,

    #[serde(rename = "type")]
    pub plugin_type: SimpleType,
}

/// Flags set when a plugin is selected.
#[derive(Debug, Clone, Deserialize)]
pub struct ConditionFlagList {
    #[serde(rename = "flag", default)]
    pub flags: Vec<ConditionFlag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionFlag {
    #[serde(rename = "@name")]
    pub name: String,

    #[serde(rename = "$text")]
    pub value: String,
}

/// Pattern-based conditional file installations evaluated after all steps.
#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalFileInstalls {
    pub patterns: ConditionalPatterns,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalPatterns {
    #[serde(rename = "pattern", default)]
    pub patterns: Vec<ConditionalPattern>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalPattern {
    pub dependencies: CompositeDependency,
    pub files: FileList,
}

// --- File/folder references ---

#[derive(Debug, Clone, Deserialize)]
pub struct FileList {
    #[serde(rename = "$value", default)]
    pub items: Vec<FileItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileItem {
    File(FileRef),
    Folder(FileRef),
}

impl FileItem {
    pub fn as_ref(&self) -> &FileRef {
        match self {
            FileItem::File(r) | FileItem::Folder(r) => r,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, FileItem::Folder(_))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    #[serde(rename = "@source")]
    pub source: String,

    #[serde(rename = "@destination", default)]
    pub destination: String,

    #[serde(rename = "@priority", default)]
    pub priority: i32,

    #[serde(rename = "@alwaysInstall", default)]
    pub always_install: bool,

    #[serde(rename = "@installIfUsable", default)]
    pub install_if_usable: bool,
}

// --- Enums ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GroupType {
    SelectExactlyOne,
    SelectAtMostOne,
    SelectAtLeastOne,
    SelectAll,
    SelectAny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PluginType {
    Required,
    Recommended,
    Optional,
    CouldBeUsable,
    NotUsable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SortOrder {
    Explicit,
    Ascending,
    Descending,
}

// --- Helpers ---

fn default_true() -> bool {
    true
}

fn default_neg_one() -> i32 {
    -1
}
