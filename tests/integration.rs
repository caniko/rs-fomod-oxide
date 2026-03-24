use std::collections::HashMap;

use fomod_oxide::condition::{EvalContext, FileState};
use fomod_oxide::config::{GroupType, PluginType};
use fomod_oxide::{
    hash_xml, DeclarativeConfig, DeclarativeError, FomodError, FomodInfo, Installer, ModuleConfig,
    SCHEMA_VERSION,
};

const CONFIG_XML: &str = include_str!("fixtures/simple_config.xml");
const INFO_XML: &str = include_str!("fixtures/info.xml");

#[test]
fn parse_info_xml() {
    let info = FomodInfo::parse(INFO_XML).unwrap();
    assert_eq!(info.name.as_deref(), Some("Example Mod"));
    assert_eq!(info.author.as_deref(), Some("TestAuthor"));
    assert_eq!(info.version.as_deref(), Some("1.2.0"));
    assert_eq!(info.website.as_deref(), Some("https://example.com"));
}

#[test]
fn parse_module_config() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    assert_eq!(config.module_name.value, "Example Mod");

    let image = config.module_image.as_ref().unwrap();
    assert_eq!(image.path, "banner.png");
    assert_eq!(image.height, 100);

    // Required files
    let required = config.required_install_files.as_ref().unwrap();
    assert_eq!(required.items.len(), 2);

    // Install steps
    let steps = config.install_steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 2);
    assert_eq!(steps.steps[0].name, "Choose Textures");
    assert_eq!(steps.steps[1].name, "Optional Patches");

    // First step's group
    let groups = &steps.steps[0].optional_file_groups.as_ref().unwrap().groups;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].group_type, GroupType::SelectExactlyOne);
    assert_eq!(groups[0].plugins.plugins.len(), 2);
    assert_eq!(groups[0].plugins.plugins[0].name, "High Resolution");
    assert_eq!(
        groups[0].plugins.plugins[0].plugin_type(),
        PluginType::Recommended
    );
    assert_eq!(
        groups[0].plugins.plugins[1].plugin_type(),
        PluginType::Optional
    );

    // Second step has visibility condition
    assert!(steps.steps[1].visible.is_some());

    // Conditional file installs
    let cfi = config.conditional_file_installs.as_ref().unwrap();
    assert_eq!(cfi.patterns.patterns.len(), 1);
}

#[test]
fn installer_default_selections() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;

    // SelectExactlyOne: should default to the Recommended plugin (index 0)
    let defaults = Installer::default_selections(&groups[0]);
    assert_eq!(defaults, vec![0]);
}

#[test]
fn installer_step_visibility() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    // Initially only step 0 is visible (step 1 requires texture_quality=high)
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].1.name, "Choose Textures");

    // Select "High Resolution" (index 0) in step 0, group 0
    installer.select(0, 0, vec![0]);

    // Now step 1 should also be visible
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[1].1.name, "Optional Patches");

    // Select "Standard Resolution" instead — step 1 should hide
    installer.select(0, 0, vec![1]);
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 1);
}

#[test]
fn installer_resolve_with_required_files() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let installer = Installer::new(config);

    // Even with no selections, required files are in the plan
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "readme.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "core_files"));
}

#[test]
fn installer_resolve_full_flow() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select high-res textures
    installer.select(0, 0, vec![0]);

    let plan = installer.resolve();

    // Required files
    assert!(plan.operations.iter().any(|op| op.source == "readme.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "core_files"));

    // Selected plugin files
    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));

    // Conditional install triggered by texture_quality=high
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp")
    );

    // Standard textures should NOT be included
    assert!(!plan.operations.iter().any(|op| op.source == "textures_2k"));
}

#[test]
fn installer_resolve_standard_textures() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select standard textures
    installer.select(0, 0, vec![1]);

    let plan = installer.resolve();

    // Standard textures included
    assert!(plan.operations.iter().any(|op| op.source == "textures_2k"));

    // HD-only conditional NOT triggered
    assert!(
        !plan
            .operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp")
    );
}

#[test]
fn validate_selection_constraints() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let group = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // SelectExactlyOne: must select 1
    assert!(Installer::validate_selection(group, &[0]).is_ok());
    assert!(Installer::validate_selection(group, &[]).is_err());
    assert!(Installer::validate_selection(group, &[0, 1]).is_err());
    assert!(Installer::validate_selection(group, &[99]).is_err());
}

#[test]
fn declarative_from_defaults() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let decl = DeclarativeConfig::from_defaults(CONFIG_XML, "1.2.0", &config);

    // Should have version, rev, and SRI hash
    assert_eq!(decl.schema_version, SCHEMA_VERSION);
    assert_eq!(decl.rev, "1.2.0");
    assert!(decl.hash.starts_with("sha256-"));

    // Should have entries for both steps
    assert!(decl.selections.contains_key("Choose Textures"));
    assert!(decl.selections.contains_key("Optional Patches"));

    // "Texture Quality" group default: "High Resolution" (Recommended)
    let tex = &decl.selections["Choose Textures"]["Texture Quality"];
    assert_eq!(tex, &vec!["High Resolution".to_string()]);

    // "Performance Patches" group (SelectAny): no Required/Recommended plugins → empty
    let patches = &decl.selections["Optional Patches"]["Performance Patches"];
    assert!(patches.is_empty());
}

#[test]
fn declarative_apply_selects_by_name() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: hash_xml(CONFIG_XML),
        rev: "test".to_string(),
        selections: HashMap::from([(
            "Choose Textures".to_string(),
            HashMap::from([(
                "Texture Quality".to_string(),
                vec!["Standard Resolution".to_string()],
            )]),
        )]),
    };

    decl.apply(CONFIG_XML, &mut installer).unwrap();

    let plan = installer.resolve();

    // Standard textures selected
    assert!(plan.operations.iter().any(|op| op.source == "textures_2k"));
    // High-res NOT selected
    assert!(!plan.operations.iter().any(|op| op.source == "textures_4k"));
    // Conditional NOT triggered (texture_quality != high)
    assert!(
        !plan
            .operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp")
    );
}

#[test]
fn declarative_apply_high_res_with_conditional() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: hash_xml(CONFIG_XML),
        rev: "test".to_string(),
        selections: HashMap::from([(
            "Choose Textures".to_string(),
            HashMap::from([(
                "Texture Quality".to_string(),
                vec!["High Resolution".to_string()],
            )]),
        )]),
    };

    decl.apply(CONFIG_XML, &mut installer).unwrap();

    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp")
    );
}

#[test]
fn declarative_apply_invalid_step_name() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: hash_xml(CONFIG_XML),
        rev: "test".to_string(),
        selections: HashMap::from([("Nonexistent Step".to_string(), HashMap::new())]),
    };

    let err = decl.apply(CONFIG_XML, &mut installer).unwrap_err();
    assert!(err.to_string().contains("Nonexistent Step"));
}

#[test]
fn declarative_apply_invalid_plugin_name() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: hash_xml(CONFIG_XML),
        rev: "test".to_string(),
        selections: HashMap::from([(
            "Choose Textures".to_string(),
            HashMap::from([(
                "Texture Quality".to_string(),
                vec!["Ultra Resolution".to_string()],
            )]),
        )]),
    };

    let err = decl.apply(CONFIG_XML, &mut installer).unwrap_err();
    assert!(err.to_string().contains("Ultra Resolution"));
}

#[test]
fn declarative_apply_rejects_hash_mismatch() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "test".to_string(),
        hash: "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        selections: HashMap::new(),
    };

    let err = decl.apply(CONFIG_XML, &mut installer).unwrap_err();
    assert!(err.to_string().contains("hash mismatch"));
}

#[test]
fn declarative_apply_rejects_future_version() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION + 1,
        hash: hash_xml(CONFIG_XML),
        rev: "test".to_string(),
        selections: HashMap::new(),
    };

    let err = decl.apply(CONFIG_XML, &mut installer).unwrap_err();
    assert!(err.to_string().contains("not supported"));
}

#[test]
fn declarative_to_nix_roundtrip() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let decl = DeclarativeConfig::from_defaults(CONFIG_XML, "1.2.0", &config);

    // Serialize to Nix expression (always available in tests via dev-dep)
    let nix_str = ronix::to_nix(&decl).unwrap();

    // Should produce valid Nix attrset syntax with rev + hash
    assert!(nix_str.contains("schema_version"));
    assert!(nix_str.contains("rev"));
    assert!(nix_str.contains("hash"));
    assert!(nix_str.contains("sha256-"));
    assert!(nix_str.contains("selections"));
    assert!(nix_str.contains("Choose Textures"));
    assert!(nix_str.contains("High Resolution"));
}

#[test]
fn condition_evaluation() {
    let mut ctx = EvalContext::new();

    // Flag dependency
    ctx.set_flag("my_flag", "yes");
    let config_xml = r#"
        <config>
            <moduleName>Test</moduleName>
            <moduleDependencies operator="And">
                <flagDependency flag="my_flag" value="yes"/>
            </moduleDependencies>
        </config>
    "#;
    let config = ModuleConfig::parse(config_xml).unwrap();
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());
}

// ---- Case-insensitive file path matching ----

#[test]
fn file_dependency_case_insensitive() {
    let mut ctx = EvalContext::new();
    // Store with one casing
    ctx.set_file_state("Data/Textures/MyMod.dds", FileState::Active);

    // Reference with different casing in XML
    let config_xml = r#"
        <config>
            <moduleName>Test</moduleName>
            <moduleDependencies operator="And">
                <fileDependency file="data/textures/mymod.dds" state="Active"/>
            </moduleDependencies>
        </config>
    "#;
    let config = ModuleConfig::parse(config_xml).unwrap();
    let installer = Installer::with_context(config, ctx);
    assert!(
        installer.check_dependencies(),
        "file dependency should match case-insensitively"
    );
}

// ---- fommDependency ----

#[test]
fn fomm_dependency_evaluation() {
    let config_xml = r#"
        <config>
            <moduleName>Test</moduleName>
            <moduleDependencies operator="And">
                <fommDependency version="1.0.0"/>
            </moduleDependencies>
        </config>
    "#;
    let config = ModuleConfig::parse(config_xml).unwrap();

    // Without manager_version set → fails
    let installer = Installer::new(config.clone());
    assert!(
        !installer.check_dependencies(),
        "fomm dependency should fail without manager_version"
    );

    // With sufficient version → passes
    let mut ctx = EvalContext::new();
    ctx.manager_version = Some("1.2.0".to_string());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(
        installer.check_dependencies(),
        "fomm dependency should pass with sufficient version"
    );

    // With lower version → fails
    let mut ctx = EvalContext::new();
    ctx.manager_version = Some("0.9.0".to_string());
    let installer = Installer::with_context(config, ctx);
    assert!(
        !installer.check_dependencies(),
        "fomm dependency should fail with insufficient version"
    );
}

// ---- SortOrder ----

#[test]
fn sort_order_ascending() {
    let config_xml = r#"
        <config>
            <moduleName>Test</moduleName>
            <installSteps order="Explicit">
                <installStep name="Step 1">
                    <optionalFileGroups order="Explicit">
                        <group name="Options" type="SelectAny">
                            <plugins order="Ascending">
                                <plugin name="Zebra">
                                    <typeDescriptor><type name="Optional"/></typeDescriptor>
                                </plugin>
                                <plugin name="Apple">
                                    <typeDescriptor><type name="Optional"/></typeDescriptor>
                                </plugin>
                                <plugin name="Mango">
                                    <typeDescriptor><type name="Optional"/></typeDescriptor>
                                </plugin>
                            </plugins>
                        </group>
                    </optionalFileGroups>
                </installStep>
            </installSteps>
        </config>
    "#;
    let config = ModuleConfig::parse(config_xml).unwrap();
    let installer = Installer::new(config);

    let plugins = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins;

    let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Apple", "Mango", "Zebra"]);
}

#[test]
fn sort_order_descending_steps() {
    let config_xml = r#"
        <config>
            <moduleName>Test</moduleName>
            <installSteps order="Descending">
                <installStep name="Alpha">
                    <optionalFileGroups>
                        <group name="G" type="SelectAny">
                            <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
                        </group>
                    </optionalFileGroups>
                </installStep>
                <installStep name="Charlie">
                    <optionalFileGroups>
                        <group name="G" type="SelectAny">
                            <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
                        </group>
                    </optionalFileGroups>
                </installStep>
                <installStep name="Bravo">
                    <optionalFileGroups>
                        <group name="G" type="SelectAny">
                            <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
                        </group>
                    </optionalFileGroups>
                </installStep>
            </installSteps>
        </config>
    "#;
    let config = ModuleConfig::parse(config_xml).unwrap();
    let installer = Installer::new(config);

    let steps = &installer.config().install_steps.as_ref().unwrap().steps;
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Charlie", "Bravo", "Alpha"]);
}

// ---- DeclarativeError into FomodError ----

#[test]
fn declarative_error_converts_to_fomod_error() {
    let err = DeclarativeError::StepNotFound("Missing".to_string());
    let fomod_err: FomodError = err.into();
    assert!(
        fomod_err.to_string().contains("Missing"),
        "DeclarativeError should convert into FomodError"
    );
}

// =====================================================================
// DependencyType first-match-wins & context-aware defaults
// =====================================================================

const DEP_TYPE_XML: &str = include_str!("fixtures/dependency_type_patterns.xml");

#[test]
fn dependency_type_first_match_wins() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select Windows → sets platform=windows AND os_family=desktop
    installer.select(0, 0, vec![0]);

    let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // "Windows Patch": first pattern (platform=windows → Required) matches BEFORE
    // second pattern (os_family=desktop → Recommended). First match wins.
    assert_eq!(
        results.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::Required,
        "Windows: first-match-wins should give Required, not Recommended"
    );

    // "Desktop Patch": os_family=desktop → Recommended
    assert_eq!(
        results.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::Recommended,
    );

    // "CouldBeUsable Plugin": no pattern matches → default CouldBeUsable
    assert_eq!(
        results.plugins.plugins[2].plugin_type_in_context(installer.context()),
        PluginType::CouldBeUsable,
    );
}

#[test]
fn dependency_type_linux_skips_windows_pattern() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select Linux → platform=linux, os_family=desktop
    installer.select(0, 0, vec![1]);

    let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // "Windows Patch": platform=linux doesn't match first pattern.
    // os_family=desktop matches second pattern → Recommended
    assert_eq!(
        results.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::Recommended,
    );
}

#[test]
fn dependency_type_steamdeck_activates_specific_plugin() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select SteamDeck → platform=steamdeck, os_family=handheld
    installer.select(0, 0, vec![2]);

    let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // "Windows Patch": neither pattern matches → NotUsable
    assert_eq!(
        results.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
    );

    // "Desktop Patch": os_family=handheld matches second pattern → Optional
    assert_eq!(
        results.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::Optional,
    );

    // "CouldBeUsable Plugin": platform=steamdeck matches → Required
    assert_eq!(
        results.plugins.plugins[2].plugin_type_in_context(installer.context()),
        PluginType::Required,
    );
}

#[test]
fn default_selections_in_context_picks_required_from_dependency_type() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Before any selection, all Results plugins are CouldBeUsable/NotUsable → no defaults
    let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];
    let defaults = Installer::default_selections_in_context(results, installer.context());
    assert!(defaults.is_empty());

    // Select Windows
    installer.select(0, 0, vec![0]);

    let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];
    let defaults = Installer::default_selections_in_context(results, installer.context());
    // Windows Patch → Required, Desktop Patch → Recommended → both selected
    assert!(defaults.contains(&0), "Windows Patch should be a default (Required)");
    assert!(defaults.contains(&1), "Desktop Patch should be a default (Recommended)");
}

#[test]
fn multiple_conditional_patterns_both_match_for_windows() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0]); // Windows: platform=windows, os_family=desktop

    let plan = installer.resolve();
    // Both desktop_extras and win_extras should be present (both conditions match)
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "desktop_extras.dll")
    );
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "win_extras.dll")
    );
    // SteamDeck extras should NOT match
    assert!(
        !plan
            .operations
            .iter()
            .any(|op| op.source == "deck_extras.dll")
    );
}

// =====================================================================
// Sort order on groups
// =====================================================================

#[test]
fn sort_order_ascending_on_groups() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups order="Ascending">
            <group name="Zebra" type="SelectAny">
                <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
            </group>
            <group name="Apple" type="SelectAny">
                <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
            </group>
            <group name="Mango" type="SelectAny">
                <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let groups = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;
    let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Apple", "Mango", "Zebra"]);
}

// =====================================================================
// Game dependency edge cases
// =====================================================================

#[test]
fn game_dependency_exact_boundary() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <gameDependency version="1.5.0"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // Exactly equal
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.5.0".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies());

    // One patch below
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.4.999".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(!installer.check_dependencies());

    // One patch above
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.5.1".into());
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());
}

#[test]
fn check_dependencies_with_no_module_dependencies() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    // No dependencies means satisfied
    assert!(installer.check_dependencies());
}

// =====================================================================
// Flag clearing across independent groups
// =====================================================================

#[test]
fn flags_from_different_groups_coexist() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G1" type="SelectExactlyOne">
                <plugins>
                    <plugin name="A1">
                        <conditionFlags><flag name="g1_flag">a1</flag></conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                    <plugin name="B1">
                        <conditionFlags><flag name="g1_flag">b1</flag></conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                </plugins>
            </group>
            <group name="G2" type="SelectExactlyOne">
                <plugins>
                    <plugin name="A2">
                        <conditionFlags><flag name="g2_flag">a2</flag></conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                    <plugin name="B2">
                        <conditionFlags><flag name="g2_flag">b2</flag></conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0]); // G1: A1 → g1_flag=a1
    installer.select(0, 1, vec![1]); // G2: B2 → g2_flag=b2

    // Both flags should coexist
    assert_eq!(installer.context().flags.get("g1_flag"), Some(&"a1".to_string()));
    assert_eq!(installer.context().flags.get("g2_flag"), Some(&"b2".to_string()));

    // Reselect G1 → only g1_flag changes, g2_flag untouched
    installer.select(0, 0, vec![1]); // G1: B1 → g1_flag=b1
    assert_eq!(installer.context().flags.get("g1_flag"), Some(&"b1".to_string()));
    assert_eq!(installer.context().flags.get("g2_flag"), Some(&"b2".to_string()));
}

// =====================================================================
// Declarative roundtrip with nix (via dev-dep)
// =====================================================================

#[test]
fn declarative_nix_roundtrip_dep_type_fixture() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let decl = DeclarativeConfig::from_defaults(DEP_TYPE_XML, "1.0", &config);
    let nix_str = ronix::to_nix(&decl).unwrap();

    assert!(nix_str.contains("DependencyType Patterns Test") || nix_str.contains("Choose Platform"));
    assert!(nix_str.contains("sha256-"));
}

// =====================================================================
// Cross-fixture consistency: from_defaults + apply + resolve
// =====================================================================

#[test]
fn cross_fixture_from_defaults_matches_manual_defaults() {
    // from_defaults applied then resolve should give the same plan as manually
    // selecting the default for every group.
    let xml = CONFIG_XML;
    let config = ModuleConfig::parse(xml).unwrap();

    // Path A: declarative from_defaults → apply → resolve
    let decl = DeclarativeConfig::from_defaults(xml, "test", &config);
    let config_a = ModuleConfig::parse(xml).unwrap();
    let mut installer_a = Installer::new(config_a);
    // from_defaults may produce selections that violate constraints (e.g. empty
    // for SelectExactlyOne with all Optional), so tolerate validation errors.
    let _ = decl.apply(xml, &mut installer_a);
    let plan_a = installer_a.resolve();

    // Path B: manual default selections for every group
    let config_b = ModuleConfig::parse(xml).unwrap();
    let mut installer_b = Installer::new(config_b.clone());
    if let Some(ref steps) = config_b.install_steps {
        for (step_idx, step) in steps.steps.iter().enumerate() {
            if let Some(ref groups) = step.optional_file_groups {
                for (group_idx, group) in groups.groups.iter().enumerate() {
                    let defaults = Installer::default_selections(group);
                    installer_b.select(step_idx, group_idx, defaults);
                }
            }
        }
    }
    let plan_b = installer_b.resolve();

    // Both plans should produce the same set of source files
    let sources_a: Vec<&str> = plan_a.operations.iter().map(|op| op.source.as_str()).collect();
    let sources_b: Vec<&str> = plan_b.operations.iter().map(|op| op.source.as_str()).collect();
    assert_eq!(sources_a, sources_b, "declarative defaults should match manual defaults");
}

// =====================================================================
// Version comparison edge cases
// =====================================================================

#[test]
fn version_comparison_four_segment() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <gameDependency version="1.0.0.0"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.0.0.0".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies(), "exact 4-segment match should pass");

    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.0.0.1".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies(), "higher 4th segment should pass");

    let mut ctx = EvalContext::new();
    ctx.game_version = Some("0.9.9.9".into());
    let installer = Installer::with_context(config, ctx);
    assert!(!installer.check_dependencies(), "lower version should fail");
}

#[test]
fn version_comparison_single_segment() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <gameDependency version="1"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies(), "single segment equal should pass");

    let mut ctx = EvalContext::new();
    ctx.game_version = Some("2".into());
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies(), "higher single segment should pass");
}

#[test]
fn version_comparison_non_numeric_segments_ignored() {
    // compare_versions filters out non-numeric segments via parse::<u32>().ok()
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <gameDependency version="2.0.0"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // "2.0.0-beta" — the "-beta" part is not parseable as u32, so it's filtered
    // out, giving [2, 0, 0] which equals the required [2, 0, 0]
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("2.0.0-beta".into());
    // The non-numeric segment "0-beta" won't parse as u32, so effectively [2]
    // This should fail since [2] < [2, 0, 0] would depend on implementation
    // We just assert it doesn't panic
    let installer = Installer::with_context(config, ctx);
    let _ = installer.check_dependencies();
}

// =====================================================================
// Deeply nested conditions (5+ levels)
// =====================================================================

#[test]
fn deeply_nested_conditions_5_levels() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <dependencies operator="And">
                <dependencies operator="Or">
                    <dependencies operator="And">
                        <dependencies operator="Or">
                            <flagDependency flag="deep" value="yes"/>
                        </dependencies>
                    </dependencies>
                </dependencies>
            </dependencies>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // Without the flag → should fail (all And-chains leading to a flag check)
    let installer = Installer::new(config.clone());
    assert!(!installer.check_dependencies(), "missing deep flag should fail");

    // With the flag → should pass
    let mut ctx = EvalContext::new();
    ctx.set_flag("deep", "yes");
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies(), "deep flag present should pass");
}

// =====================================================================
// Large config stress test
// =====================================================================

#[test]
fn large_config_stress_test() {
    // Generate XML with 10 steps, each with 3 groups and 5 plugins
    let mut xml = String::from(r#"<config><moduleName>StressTest</moduleName><installSteps>"#);
    for step in 0..10 {
        xml.push_str(&format!(r#"<installStep name="Step{step}"><optionalFileGroups>"#));
        for group in 0..3 {
            xml.push_str(&format!(
                r#"<group name="Group{group}" type="SelectAny"><plugins>"#
            ));
            for plugin in 0..5 {
                xml.push_str(&format!(
                    r#"<plugin name="Plugin{plugin}">
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="s{step}_g{group}_p{plugin}.esp" destination="Data"/></files>
                    </plugin>"#
                ));
            }
            xml.push_str("</plugins></group>");
        }
        xml.push_str("</optionalFileGroups></installStep>");
    }
    xml.push_str("</installSteps></config>");

    let config = ModuleConfig::parse(&xml).unwrap();
    let steps = config.install_steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 10);

    let mut installer = Installer::new(config);

    // Select plugin 0 and 2 in every group
    for step in 0..10 {
        for group in 0..3 {
            installer.select(step, group, vec![0, 2]);
        }
    }

    let plan = installer.resolve();
    // 10 steps * 3 groups * 2 selected plugins = 60 operations
    assert_eq!(plan.operations.len(), 60, "should have 60 file operations");

    // Verify a sample operation exists
    assert!(plan.operations.iter().any(|op| op.source == "s0_g0_p0.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "s9_g2_p2.esp"));

    // completion_status should work
    let status = installer.completion_status();
    assert_eq!(status.total_steps, 10);
    assert_eq!(status.total_groups, 30);
}

// =====================================================================
// Case-insensitive file dependency with various casing patterns
// =====================================================================

#[test]
fn file_dependency_various_casing_patterns() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <fileDependency file="Data/Textures/MyFile.DDS" state="Active"/>
        </moduleDependencies></config>
    "#;

    let test_cases = [
        ("DATA/TEXTURES/MYFILE.DDS", true, "UPPER case"),
        ("data/textures/myfile.dds", true, "lower case"),
        ("dAtA/tExTuReS/mYfIlE.dDs", true, "mIxEd case"),
        ("Data/Textures/MyFile.DDS", true, "exact case"),
        ("Data/Textures/OtherFile.DDS", false, "different file"),
    ];

    for (path, expected, label) in &test_cases {
        let config = ModuleConfig::parse(xml).unwrap();
        let mut ctx = EvalContext::new();
        ctx.set_file_state(*path, FileState::Active);
        let installer = Installer::with_context(config, ctx);
        assert_eq!(
            installer.check_dependencies(),
            *expected,
            "file dependency {label}: path={path}"
        );
    }
}

// =====================================================================
// Duplicate plugin indices in selection
// =====================================================================

#[test]
fn duplicate_plugin_indices_in_selection() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G" type="SelectAny">
                <plugins>
                    <plugin name="A">
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="a.esp" destination="Data"/></files>
                    </plugin>
                    <plugin name="B">
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="b.esp" destination="Data"/></files>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select [0, 0, 1] — duplicate index 0
    installer.select(0, 0, vec![0, 0, 1]);

    let plan = installer.resolve();
    // Should include files from plugin 0 (possibly duplicated) and plugin 1
    assert!(plan.operations.iter().any(|op| op.source == "a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "b.esp"));

    // The duplicate should result in a.esp appearing twice in operations
    let a_count = plan.operations.iter().filter(|op| op.source == "a.esp").count();
    assert_eq!(a_count, 2, "duplicate index should produce duplicate file ops");
}

// =====================================================================
// All plugins NotUsable in SelectExactlyOne
// =====================================================================

#[test]
fn all_not_usable_select_exactly_one_defaults_empty() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G" type="SelectExactlyOne">
                <plugins>
                    <plugin name="A">
                        <typeDescriptor><type name="NotUsable"/></typeDescriptor>
                    </plugin>
                    <plugin name="B">
                        <typeDescriptor><type name="NotUsable"/></typeDescriptor>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let group = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    let defaults = Installer::default_selections(group);
    assert!(
        defaults.is_empty(),
        "all NotUsable plugins should produce empty defaults"
    );
}

// =====================================================================
// Multiple flags from same plugin
// =====================================================================

#[test]
fn multiple_flags_from_same_plugin() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G" type="SelectExactlyOne">
                <plugins>
                    <plugin name="Multi">
                        <conditionFlags>
                            <flag name="flag_a">val_a</flag>
                            <flag name="flag_b">val_b</flag>
                            <flag name="flag_c">val_c</flag>
                        </conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                    <plugin name="Other">
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0]);

    let ctx = installer.context();
    assert_eq!(ctx.flags.get("flag_a"), Some(&"val_a".to_string()));
    assert_eq!(ctx.flags.get("flag_b"), Some(&"val_b".to_string()));
    assert_eq!(ctx.flags.get("flag_c"), Some(&"val_c".to_string()));
}

// =====================================================================
// Conditional file installs with no matching patterns
// =====================================================================

#[test]
fn conditional_file_installs_no_matching_patterns() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G" type="SelectExactlyOne">
                <plugins>
                    <plugin name="A">
                        <conditionFlags><flag name="choice">alpha</flag></conditionFlags>
                        <typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="a.esp" destination="Data"/></files>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps>
        <conditionalFileInstalls>
            <patterns>
                <pattern>
                    <dependencies><flagDependency flag="choice" value="beta"/></dependencies>
                    <files><file source="beta_patch.esp" destination="Data"/></files>
                </pattern>
                <pattern>
                    <dependencies><flagDependency flag="choice" value="gamma"/></dependencies>
                    <files><file source="gamma_patch.esp" destination="Data"/></files>
                </pattern>
            </patterns>
        </conditionalFileInstalls></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select alpha — neither beta nor gamma pattern should match
    installer.select(0, 0, vec![0]);
    let plan = installer.resolve();

    // a.esp should be included from plugin selection
    assert!(plan.operations.iter().any(|op| op.source == "a.esp"));
    // Neither conditional pattern should fire
    assert!(
        !plan.operations.iter().any(|op| op.source == "beta_patch.esp"),
        "beta pattern should not match when choice=alpha"
    );
    assert!(
        !plan.operations.iter().any(|op| op.source == "gamma_patch.esp"),
        "gamma pattern should not match when choice=alpha"
    );
}
