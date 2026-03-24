//! Edge case tests for XML parsing, condition evaluation, and installer behavior.

use std::collections::HashMap;

use fomod_oxide::condition::{EvalContext, FileState};
use fomod_oxide::Evaluate;
use fomod_oxide::config::{GroupType, PluginType};
use fomod_oxide::{
    hash_xml, DeclarativeConfig, DeclarativeError,
    FileConflictSource, FomodError, FomodInfo, Installer, ModuleConfig,
    SelectionSummary, ValidationHint, SCHEMA_VERSION,
};

// =====================================================================
// XML parsing edge cases
// =====================================================================

#[test]
fn parse_minimal_config_only_name() {
    let xml = r#"<config><moduleName>Minimal</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert_eq!(config.module_name.value, "Minimal");
    assert!(config.module_image.is_none());
    assert!(config.module_dependencies.is_none());
    assert!(config.required_install_files.is_none());
    assert!(config.install_steps.is_none());
    assert!(config.conditional_file_installs.is_none());
}

#[test]
fn parse_minimal_config_from_fixture() {
    let xml = include_str!("fixtures/minimal_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    assert_eq!(config.module_name.value, "Minimal");
}

#[test]
fn parse_empty_string_fails() {
    assert!(ModuleConfig::parse("").is_err());
}

#[test]
fn parse_garbage_fails() {
    assert!(ModuleConfig::parse("this is not XML").is_err());
}

#[test]
fn parse_valid_xml_wrong_structure_fails() {
    // Valid XML but missing required moduleName
    assert!(ModuleConfig::parse("<config></config>").is_err());
}

#[test]
fn parse_empty_module_name_fails() {
    // Empty <moduleName/> is invalid — quick_xml requires $text content for ModuleName
    let xml = r#"<config><moduleName></moduleName></config>"#;
    assert!(ModuleConfig::parse(xml).is_err());
}

#[test]
fn parse_special_characters_in_names() {
    let xml = r#"
        <config><moduleName>Mod &amp; Patch &lt;v2&gt;</moduleName>
        <installSteps><installStep name="Step &amp; Go">
        <optionalFileGroups><group name="O'Brien's &quot;Choice&quot;" type="SelectAny">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert_eq!(config.module_name.value, "Mod & Patch <v2>");
    let step = &config.install_steps.as_ref().unwrap().steps[0];
    assert_eq!(step.name, "Step & Go");
}

#[test]
fn parse_unicode_names() {
    let xml = r#"
        <config><moduleName>日本語MOD テスト</moduleName>
        <installSteps><installStep name="步骤一">
        <optionalFileGroups><group name="Группа" type="SelectAny">
        <plugins><plugin name="プラグイン"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert_eq!(config.module_name.value, "日本語MOD テスト");
}

#[test]
fn parse_empty_required_files() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles></requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert!(config.required_install_files.as_ref().unwrap().items.is_empty());
}

#[test]
fn parse_empty_install_steps() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert!(config.install_steps.as_ref().unwrap().steps.is_empty());
}

#[test]
fn parse_plugin_no_type_descriptor() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="Bare"></plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert_eq!(plugin.plugin_type(), PluginType::Optional);
}

#[test]
fn parse_plugin_no_files() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="NoFiles">
            <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert!(plugin.files.is_none());
}

#[test]
fn parse_empty_files_element() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <typeDescriptor><type name="Optional"/></typeDescriptor>
            <files/>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert!(plugin.files.as_ref().unwrap().items.is_empty());
}

#[test]
fn parse_file_ref_all_attributes() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <file source="mod.esp" destination="Data" priority="42" alwaysInstall="true" installIfUsable="true"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let files = config.required_install_files.as_ref().unwrap();
    let r = files.items[0].file_ref();
    assert_eq!(r.source, "mod.esp");
    assert_eq!(r.destination, "Data");
    assert_eq!(r.priority, 42);
    assert!(r.always_install);
    assert!(r.install_if_usable);
}

#[test]
fn parse_file_ref_defaults() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <file source="mod.esp"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let files = config.required_install_files.as_ref().unwrap();
    let r = files.items[0].file_ref();
    assert_eq!(r.destination, "");
    assert_eq!(r.priority, 0);
    assert!(!r.always_install);
    assert!(!r.install_if_usable);
}

#[test]
fn parse_mixed_file_and_folder() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <file source="a.esp" destination="Data"/>
            <folder source="meshes" destination="Data/meshes"/>
            <file source="b.esp" destination="Data"/>
            <folder source="textures"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let items = &config.required_install_files.as_ref().unwrap().items;
    assert_eq!(items.len(), 4);
    assert!(!items[0].is_folder());
    assert!(items[1].is_folder());
    assert!(!items[2].is_folder());
    assert!(items[3].is_folder());
}

// =====================================================================
// Condition evaluation edge cases
// =====================================================================

#[test]
fn nested_and_or_conditions() {
    let xml = include_str!("fixtures/nested_conditions.xml");
    let config = ModuleConfig::parse(xml).unwrap();

    // Module dep: (flagA=yes AND (flagB=yes OR flagC=yes))
    let deps = config.module_dependencies.as_ref().unwrap();

    // Nothing set → false
    let ctx = EvalContext::new();
    assert!(!deps.evaluate(&ctx));

    // Only flagA → false (inner OR fails)
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    assert!(!deps.evaluate(&ctx));

    // flagA + flagB → true
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    ctx.set_flag("flagB", "yes");
    assert!(deps.evaluate(&ctx));

    // flagA + flagC → true
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    ctx.set_flag("flagC", "yes");
    assert!(deps.evaluate(&ctx));

    // Only flagB → false (outer AND fails on flagA)
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagB", "yes");
    assert!(!deps.evaluate(&ctx));
}

#[test]
fn deeply_nested_conditional_file_installs() {
    // The nested_conditions.xml has a pattern: (A AND (B OR (C AND D)))
    let xml = include_str!("fixtures/nested_conditions.xml");
    let config = ModuleConfig::parse(xml).unwrap();

    let pattern = &config.conditional_file_installs.as_ref().unwrap().patterns.patterns[0];

    // A + B → true (B satisfies the OR)
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    ctx.set_flag("flagB", "yes");
    assert!(pattern.dependencies.evaluate(&ctx));

    // A + C + D → true (C AND D satisfies the inner AND, which satisfies the OR)
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    ctx.set_flag("flagC", "yes");
    ctx.set_flag("flagD", "yes");
    assert!(pattern.dependencies.evaluate(&ctx));

    // A + C only → false (C AND D needs both)
    let mut ctx = EvalContext::new();
    ctx.set_flag("flagA", "yes");
    ctx.set_flag("flagC", "yes");
    assert!(!pattern.dependencies.evaluate(&ctx));

    // None → false
    let ctx = EvalContext::new();
    assert!(!pattern.dependencies.evaluate(&ctx));
}

#[test]
fn file_dependency_all_states() {
    let base_xml = |state: &str| {
        format!(
            r#"<config><moduleName>T</moduleName>
            <moduleDependencies>
                <fileDependency file="mod.esp" state="{state}"/>
            </moduleDependencies></config>"#
        )
    };

    // Active
    let config = ModuleConfig::parse(&base_xml("Active")).unwrap();
    let mut ctx = EvalContext::new();
    ctx.set_file_state("mod.esp", FileState::Active);
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());

    // Inactive
    let config = ModuleConfig::parse(&base_xml("Inactive")).unwrap();
    let mut ctx = EvalContext::new();
    ctx.set_file_state("mod.esp", FileState::Inactive);
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());

    // Missing
    let config = ModuleConfig::parse(&base_xml("Missing")).unwrap();
    let ctx = EvalContext::new(); // mod.esp not in context → Missing
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());
}

#[test]
fn game_dependency_evaluation() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <gameDependency version="1.5.0"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // No game version → fails
    let installer = Installer::new(config.clone());
    assert!(!installer.check_dependencies());

    // Sufficient version
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.5.0".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies());

    // Higher version
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("2.0.0".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies());

    // Lower version
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.4.9".into());
    let installer = Installer::with_context(config, ctx);
    assert!(!installer.check_dependencies());
}

#[test]
fn mixed_dependency_types_in_composite() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <flagDependency flag="enabled" value="yes"/>
            <fileDependency file="base.esm" state="Active"/>
            <gameDependency version="1.0"/>
            <fommDependency version="2.0"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // All satisfied
    let mut ctx = EvalContext::new();
    ctx.set_flag("enabled", "yes");
    ctx.set_file_state("base.esm", FileState::Active);
    ctx.game_version = Some("1.0".into());
    ctx.manager_version = Some("2.0".into());
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies());

    // One missing (no manager version)
    let mut ctx = EvalContext::new();
    ctx.set_flag("enabled", "yes");
    ctx.set_file_state("base.esm", FileState::Active);
    ctx.game_version = Some("1.0".into());
    let installer = Installer::with_context(config, ctx);
    assert!(!installer.check_dependencies());
}

#[test]
fn or_dependency_any_satisfied() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="Or">
            <flagDependency flag="a" value="yes"/>
            <flagDependency flag="b" value="yes"/>
            <flagDependency flag="c" value="yes"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // Only b set
    let mut ctx = EvalContext::new();
    ctx.set_flag("b", "yes");
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());
}

// =====================================================================
// All group types
// =====================================================================

#[test]
fn all_group_types_parse_and_validate() {
    let xml = include_str!("fixtures/all_group_types.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;

    assert_eq!(groups.len(), 5);
    assert_eq!(groups[0].group_type, GroupType::SelectExactlyOne);
    assert_eq!(groups[1].group_type, GroupType::SelectAtMostOne);
    assert_eq!(groups[2].group_type, GroupType::SelectAtLeastOne);
    assert_eq!(groups[3].group_type, GroupType::SelectAll);
    assert_eq!(groups[4].group_type, GroupType::SelectAny);

    // Validate correct selections for each type
    assert!(Installer::validate_selection(&groups[0], &[0]).is_ok()); // exactly 1
    assert!(Installer::validate_selection(&groups[1], &[]).is_ok()); // at most 1: 0 ok
    assert!(Installer::validate_selection(&groups[1], &[0]).is_ok()); // at most 1: 1 ok
    assert!(Installer::validate_selection(&groups[2], &[0, 1]).is_ok()); // at least 1
    assert!(Installer::validate_selection(&groups[3], &[0, 1]).is_ok()); // all
    assert!(Installer::validate_selection(&groups[4], &[]).is_ok()); // any: 0 ok
    assert!(Installer::validate_selection(&groups[4], &[0, 1, 2]).is_ok()); // any: all ok
}

#[test]
fn all_group_types_invalid_selections() {
    let xml = include_str!("fixtures/all_group_types.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;

    // ExactlyOne: 0 and 2 fail
    assert!(Installer::validate_selection(&groups[0], &[]).is_err());
    assert!(Installer::validate_selection(&groups[0], &[0, 1]).is_err());

    // AtMostOne: 2 fails
    assert!(Installer::validate_selection(&groups[1], &[0, 1]).is_err());

    // AtLeastOne: 0 fails
    assert!(Installer::validate_selection(&groups[2], &[]).is_err());

    // All: partial fails
    assert!(Installer::validate_selection(&groups[3], &[0]).is_err());
}

#[test]
fn all_group_types_default_selections() {
    let xml = include_str!("fixtures/all_group_types.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;

    // ExactlyOne: Required plugin at index 0
    assert_eq!(Installer::default_selections(&groups[0]), vec![0]);

    // AtMostOne: Recommended at index 0
    assert_eq!(Installer::default_selections(&groups[1]), vec![0]);

    // AtLeastOne: Required(0) + Recommended(1)
    assert_eq!(Installer::default_selections(&groups[2]), vec![0, 1]);

    // All: all indices
    assert_eq!(Installer::default_selections(&groups[3]), vec![0, 1]);

    // Any: no Required/Recommended → empty
    assert!(Installer::default_selections(&groups[4]).is_empty());
}

// =====================================================================
// Priority and file handling
// =====================================================================

#[test]
fn priority_ordering_in_resolve() {
    let xml = include_str!("fixtures/priority_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0, 1, 2]); // all plugins

    let plan = installer.resolve();
    let priorities: Vec<i32> = plan.operations.iter().map(|op| op.priority).collect();
    // Should be sorted ascending
    assert!(priorities.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn empty_destination_preserved() {
    let xml = include_str!("fixtures/priority_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![2]); // "No Destination" plugin

    let plan = installer.resolve();
    let no_dest_ops: Vec<_> = plan
        .operations
        .iter()
        .filter(|op| op.source == "keep_path.esp" || op.source == "keep_folder")
        .collect();
    assert_eq!(no_dest_ops.len(), 2);
    for op in &no_dest_ops {
        assert_eq!(op.destination, "", "empty destination should be preserved");
    }
}

#[test]
fn folder_vs_file_flag_in_operations() {
    let xml = include_str!("fixtures/priority_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0, 2]); // Low Priority + No Destination

    let plan = installer.resolve();
    let folder_ops: Vec<_> = plan.operations.iter().filter(|op| op.is_folder).collect();
    let file_ops: Vec<_> = plan.operations.iter().filter(|op| !op.is_folder).collect();

    assert!(folder_ops.len() >= 2); // low_folder + keep_folder
    assert!(file_ops.len() >= 3); // base.esp + base_override.esp + low.esp + keep_path.esp
}

// =====================================================================
// Step visibility chains
// =====================================================================

#[test]
fn multi_step_visibility_basic_mode() {
    let xml = include_str!("fixtures/multi_step_visibility.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select "Basic" mode
    installer.select(0, 0, vec![0]);

    let visible: Vec<&str> = installer
        .visible_steps()
        .iter()
        .map(|(_, s)| s.name.as_str())
        .collect();

    assert!(visible.contains(&"Choose Mode"));
    assert!(!visible.contains(&"Advanced Options"));
    assert!(!visible.contains(&"Expert Options"));
    assert!(!visible.contains(&"Summary")); // Summary needs advanced or expert
}

#[test]
fn multi_step_visibility_advanced_mode() {
    let xml = include_str!("fixtures/multi_step_visibility.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select "Advanced" mode
    installer.select(0, 0, vec![1]);

    let visible: Vec<&str> = installer
        .visible_steps()
        .iter()
        .map(|(_, s)| s.name.as_str())
        .collect();

    assert!(visible.contains(&"Choose Mode"));
    assert!(visible.contains(&"Advanced Options"));
    assert!(!visible.contains(&"Expert Options"));
    assert!(visible.contains(&"Summary")); // OR: mode=advanced
}

#[test]
fn multi_step_visibility_expert_mode() {
    let xml = include_str!("fixtures/multi_step_visibility.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select "Expert" mode (sets mode=expert AND show_expert=yes)
    installer.select(0, 0, vec![2]);

    let visible: Vec<&str> = installer
        .visible_steps()
        .iter()
        .map(|(_, s)| s.name.as_str())
        .collect();

    assert!(visible.contains(&"Choose Mode"));
    assert!(!visible.contains(&"Advanced Options"));
    assert!(visible.contains(&"Expert Options")); // AND: mode=expert + show_expert=yes
    assert!(visible.contains(&"Summary")); // OR: mode=expert
}

#[test]
fn step_visibility_toggles_with_reselection() {
    let xml = include_str!("fixtures/multi_step_visibility.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Start with expert
    installer.select(0, 0, vec![2]);
    assert_eq!(installer.visible_steps().len(), 3); // Choose + Expert + Summary

    // Switch to basic → only Choose visible
    installer.select(0, 0, vec![0]);
    assert_eq!(installer.visible_steps().len(), 1);

    // Switch to advanced
    installer.select(0, 0, vec![1]);
    assert_eq!(installer.visible_steps().len(), 3); // Choose + Advanced + Summary
}

// =====================================================================
// Installer flow edge cases
// =====================================================================

#[test]
fn select_from_nonexistent_step_is_harmless() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Selecting from nonexistent step/group shouldn't panic
    installer.select(99, 0, vec![0]);
    installer.select(0, 99, vec![0]);

    let plan = installer.resolve();
    assert!(plan.operations.is_empty());
}

#[test]
fn multiple_selections_across_groups() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups>
            <group name="G1" type="SelectExactlyOne">
                <plugins>
                    <plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="g1a.esp" destination="Data"/></files>
                    </plugin>
                    <plugin name="B"><typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="g1b.esp" destination="Data"/></files>
                    </plugin>
                </plugins>
            </group>
            <group name="G2" type="SelectAny">
                <plugins>
                    <plugin name="C"><typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="g2c.esp" destination="Data"/></files>
                    </plugin>
                    <plugin name="D"><typeDescriptor><type name="Optional"/></typeDescriptor>
                        <files><file source="g2d.esp" destination="Data"/></files>
                    </plugin>
                </plugins>
            </group>
        </optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![1]); // G1: B
    installer.select(0, 1, vec![0, 1]); // G2: C + D

    let plan = installer.resolve();
    assert!(!plan.operations.iter().any(|op| op.source == "g1a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "g1b.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "g2c.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "g2d.esp"));
}

#[test]
fn reselection_replaces_previous() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectExactlyOne">
        <plugins>
            <plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="a.esp" destination="Data"/></files>
            </plugin>
            <plugin name="B"><typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="b.esp" destination="Data"/></files>
            </plugin>
        </plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0]); // select A
    installer.select(0, 0, vec![1]); // change to B

    let plan = installer.resolve();
    assert!(!plan.operations.iter().any(|op| op.source == "a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "b.esp"));
}

#[test]
fn conditional_files_depend_on_final_flags() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectExactlyOne">
        <plugins>
            <plugin name="Yes">
                <conditionFlags><flag name="feature">on</flag></conditionFlags>
                <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
            <plugin name="No">
                <conditionFlags><flag name="feature">off</flag></conditionFlags>
                <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
        </plugins>
        </group></optionalFileGroups>
        </installStep></installSteps>
        <conditionalFileInstalls><patterns>
            <pattern>
                <dependencies><flagDependency flag="feature" value="on"/></dependencies>
                <files><file source="bonus.esp" destination="Data"/></files>
            </pattern>
        </patterns></conditionalFileInstalls></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select "Yes" → feature=on → conditional triggers
    installer.select(0, 0, vec![0]);
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "bonus.esp"));

    // Reparse and select "No" → feature=off → conditional doesn't trigger
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![1]);
    let plan = installer.resolve();
    assert!(!plan.operations.iter().any(|op| op.source == "bonus.esp"));
}

// =====================================================================
// Declarative edge cases
// =====================================================================

#[test]
fn declarative_from_all_vs_defaults() {
    let xml = include_str!("fixtures/all_group_types.xml");
    let config = ModuleConfig::parse(xml).unwrap();

    let defaults = DeclarativeConfig::from_defaults(xml, "1.0", &config);
    let all = DeclarativeConfig::from_all(xml, "1.0", &config);

    // "from_all" should have more or equal plugins in every group
    for (step_name, step_groups) in &all.selections {
        for (group_name, all_plugins) in step_groups {
            let default_plugins = defaults
                .selections
                .get(step_name)
                .and_then(|s| s.get(group_name))
                .cloned()
                .unwrap_or_default();
            assert!(
                all_plugins.len() >= default_plugins.len(),
                "from_all should include >= plugins for {step_name}/{group_name}"
            );
        }
    }
}

#[test]
fn declarative_partial_selections_use_defaults() {
    let xml = include_str!("fixtures/all_group_types.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Only specify "Any" group, let others default
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "test".into(),
        hash: hash_xml(xml),
        selections: HashMap::from([(
            "Step 1".into(),
            HashMap::from([(
                "Any".into(),
                vec!["Whatever A".into(), "Whatever C".into()],
            )]),
        )]),
    };

    decl.apply(xml, &mut installer).unwrap();
    let plan = installer.resolve();

    // "Any" group: selected A and C
    assert!(plan.operations.iter().any(|op| op.source == "any_a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "any_c.esp"));
    assert!(!plan.operations.iter().any(|op| op.source == "any_b.esp"));
}

#[test]
fn declarative_apply_sets_flags_correctly() {
    let xml = include_str!("fixtures/simple_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "test".into(),
        hash: hash_xml(xml),
        selections: HashMap::from([(
            "Choose Textures".into(),
            HashMap::from([(
                "Texture Quality".into(),
                vec!["High Resolution".into()],
            )]),
        )]),
    };

    decl.apply(xml, &mut installer).unwrap();
    assert_eq!(
        installer.context().flags.get("texture_quality"),
        Some(&"high".to_string())
    );
}

// =====================================================================
// Error type conversions
// =====================================================================

#[test]
fn declarative_errors_convert_to_fomod_error() {
    let variants: Vec<DeclarativeError> = vec![
        DeclarativeError::UnsupportedVersion {
            config: 99,
            supported: 1,
        },
        DeclarativeError::HashMismatch {
            expected: "a".into(),
            actual: "b".into(),
        },
        DeclarativeError::StepNotFound("s".into()),
        DeclarativeError::GroupNotFound {
            step: "s".into(),
            group: "g".into(),
        },
        DeclarativeError::PluginNotFound {
            step: "s".into(),
            group: "g".into(),
            plugin: "p".into(),
        },
        DeclarativeError::ValidationFailed {
            step: "s".into(),
            group: "g".into(),
            message: "bad".into(),
        },
    ];

    for err in variants {
        let _display = err.to_string();
        let fomod_err: FomodError = err.into();
        // All should convert and preserve message
        assert!(!fomod_err.to_string().is_empty());
    }
}

// =====================================================================
// FomodInfo edge cases
// =====================================================================

#[test]
fn info_xml_empty_fomod() {
    let xml = "<fomod></fomod>";
    let info = FomodInfo::parse(xml).unwrap();
    assert!(info.name.is_none());
    assert!(info.author.is_none());
}

#[test]
fn info_xml_only_id() {
    let xml = r#"<fomod><Id>42</Id></fomod>"#;
    let info = FomodInfo::parse(xml).unwrap();
    assert_eq!(info.id.as_deref(), Some("42"));
    assert!(info.name.is_none());
}

#[test]
fn info_xml_invalid_fails() {
    assert!(FomodInfo::parse("garbage").is_err());
    assert!(FomodInfo::parse("").is_err());
}

// =====================================================================
// SRI hash edge cases
// =====================================================================

#[test]
fn hash_deterministic() {
    let h1 = hash_xml("test content");
    let h2 = hash_xml("test content");
    assert_eq!(h1, h2);
}

#[test]
fn hash_different_content() {
    assert_ne!(hash_xml("a"), hash_xml("b"));
}

#[test]
fn hash_whitespace_sensitive() {
    assert_ne!(hash_xml("<config/>"), hash_xml("<config />"));
    assert_ne!(hash_xml("a"), hash_xml("a "));
}

#[test]
fn hash_empty_string() {
    let h = hash_xml("");
    assert!(h.starts_with("sha256-"));
    assert!(h.len() > 10);
}

// =====================================================================
// Additional XML parsing edge cases
// =====================================================================

#[test]
fn parse_plugin_description_and_image() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <description>A really cool mod plugin</description>
            <image path="images/preview.png"/>
            <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert_eq!(
        plugin.description.as_deref(),
        Some("A really cool mod plugin")
    );
    assert_eq!(plugin.image.as_ref().unwrap().path, "images/preview.png");
}

#[test]
fn parse_name_position_all_variants() {
    for (pos, expected) in [
        ("Left", fomod_oxide::config::NamePosition::Left),
        ("Right", fomod_oxide::config::NamePosition::Right),
        (
            "RightOfImage",
            fomod_oxide::config::NamePosition::RightOfImage,
        ),
    ] {
        let xml = format!(
            r#"<config><moduleName position="{pos}">Test</moduleName></config>"#
        );
        let config = ModuleConfig::parse(&xml).unwrap();
        assert_eq!(config.module_name.position, Some(expected));
    }
}

#[test]
fn parse_name_position_absent() {
    let xml = r#"<config><moduleName>Test</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    assert!(config.module_name.position.is_none());
}

#[test]
fn parse_module_image_defaults_only_path() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleImage path="banner.png"/></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let img = config.module_image.as_ref().unwrap();
    assert_eq!(img.path, "banner.png");
    assert!(img.show_image);
    assert!(img.show_fade);
    assert_eq!(img.height, -1);
}

#[test]
fn parse_module_image_all_overridden() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleImage path="img.jpg" showImage="false" showFade="false" height="300"/></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let img = config.module_image.as_ref().unwrap();
    assert!(!img.show_image);
    assert!(!img.show_fade);
    assert_eq!(img.height, 300);
}

#[test]
fn parse_could_be_usable_plugin_type() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <typeDescriptor><type name="CouldBeUsable"/></typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert_eq!(plugin.plugin_type(), PluginType::CouldBeUsable);
}

#[test]
fn parse_dependency_type_with_patterns() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <typeDescriptor>
                <dependencyType>
                    <defaultType name="NotUsable"/>
                    <patterns>
                        <pattern>
                            <dependencies><flagDependency flag="x" value="1"/></dependencies>
                            <type name="Required"/>
                        </pattern>
                        <pattern>
                            <dependencies><flagDependency flag="y" value="2"/></dependencies>
                            <type name="Recommended"/>
                        </pattern>
                    </patterns>
                </dependencyType>
            </typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];

    // Static resolution returns default type
    assert_eq!(plugin.plugin_type(), PluginType::NotUsable);

    // Context with x=1 → Required (first pattern wins)
    let mut ctx = EvalContext::new();
    ctx.set_flag("x", "1");
    assert_eq!(plugin.plugin_type_in_context(&ctx), PluginType::Required);

    // Context with y=2 → Recommended (second pattern)
    let mut ctx = EvalContext::new();
    ctx.set_flag("y", "2");
    assert_eq!(
        plugin.plugin_type_in_context(&ctx),
        PluginType::Recommended
    );

    // Context with both x=1 and y=2 → Required (first match wins)
    let mut ctx = EvalContext::new();
    ctx.set_flag("x", "1");
    ctx.set_flag("y", "2");
    assert_eq!(plugin.plugin_type_in_context(&ctx), PluginType::Required);

    // Context with no matching flags → NotUsable (default)
    let ctx = EvalContext::new();
    assert_eq!(plugin.plugin_type_in_context(&ctx), PluginType::NotUsable);
}

#[test]
fn parse_file_ref_with_backslash_paths() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <folder source="SE_CUDA\SKSE" destination="Data\SKSE"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let item = &config.required_install_files.as_ref().unwrap().items[0];
    assert!(item.is_folder());
    assert_eq!(item.file_ref().source, "SE_CUDA\\SKSE");
    assert_eq!(item.file_ref().destination, "Data\\SKSE");
}

#[test]
fn parse_very_long_module_name() {
    let long_name = "A".repeat(10000);
    let xml = format!("<config><moduleName>{long_name}</moduleName></config>");
    let config = ModuleConfig::parse(&xml).unwrap();
    assert_eq!(config.module_name.value.len(), 10000);
}

#[test]
fn parse_empty_groups_list() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;
    assert!(groups.is_empty());
}

#[test]
fn parse_empty_plugins_list() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugins = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins;
    assert!(plugins.is_empty());
}

#[test]
fn parse_negative_priority() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <file source="low.esp" priority="-100"/>
            <file source="high.esp" priority="100"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let items = &config.required_install_files.as_ref().unwrap().items;
    assert_eq!(items[0].file_ref().priority, -100);
    assert_eq!(items[1].file_ref().priority, 100);
}

#[test]
fn parse_empty_condition_flags() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <conditionFlags></conditionFlags>
            <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let plugin = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0]
        .plugins
        .plugins[0];
    assert!(
        plugin
            .condition_flags
            .as_ref()
            .unwrap()
            .flags
            .is_empty()
    );
}

#[test]
fn parse_multiple_steps_with_same_group_names() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps>
            <installStep name="Step A">
                <optionalFileGroups><group name="Options" type="SelectAny">
                <plugins><plugin name="P1"><typeDescriptor><type name="Optional"/></typeDescriptor>
                    <files><file source="a.esp"/></files>
                </plugin></plugins>
                </group></optionalFileGroups>
            </installStep>
            <installStep name="Step B">
                <optionalFileGroups><group name="Options" type="SelectAny">
                <plugins><plugin name="P2"><typeDescriptor><type name="Optional"/></typeDescriptor>
                    <files><file source="b.esp"/></files>
                </plugin></plugins>
                </group></optionalFileGroups>
            </installStep>
        </installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select from both steps
    installer.select(0, 0, vec![0]); // Step A, group "Options"
    installer.select(1, 0, vec![0]); // Step B, group "Options"

    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "b.esp"));
}

// =====================================================================
// Additional condition evaluation edge cases
// =====================================================================

#[test]
fn flag_dependency_empty_value_matches_empty() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies>
            <flagDependency flag="cleared" value=""/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // Flag set to empty string should match
    let mut ctx = EvalContext::new();
    ctx.set_flag("cleared", "");
    let installer = Installer::with_context(config.clone(), ctx);
    assert!(installer.check_dependencies());

    // Flag not set at all should NOT match (missing != empty)
    let ctx = EvalContext::new();
    let installer = Installer::with_context(config, ctx);
    assert!(!installer.check_dependencies());
}

#[test]
fn or_dependency_none_satisfied_is_false() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="Or">
            <flagDependency flag="a" value="yes"/>
            <flagDependency flag="b" value="yes"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let ctx = EvalContext::new();
    let installer = Installer::with_context(config, ctx);
    assert!(!installer.check_dependencies());
}

#[test]
fn and_dependency_empty_is_vacuously_true() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert!(installer.check_dependencies());
}

#[test]
fn or_dependency_empty_is_false() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="Or">
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert!(!installer.check_dependencies());
}

#[test]
fn file_dependency_active_vs_inactive_vs_missing() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <fileDependency file="active.esm" state="Active"/>
            <fileDependency file="inactive.esm" state="Inactive"/>
            <fileDependency file="gone.esm" state="Missing"/>
        </moduleDependencies></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    let mut ctx = EvalContext::new();
    ctx.set_file_state("active.esm", FileState::Active);
    ctx.set_file_state("inactive.esm", FileState::Inactive);
    // gone.esm not in context → defaults to Missing
    let installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());
}

// =====================================================================
// Additional installer edge cases
// =====================================================================

#[test]
fn context_mut_allows_external_flag_setting() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <visible><flagDependency flag="unlocked" value="true"/></visible>
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Step hidden initially
    assert!(installer.visible_steps().is_empty());

    // Set flag externally via context_mut
    installer.context_mut().set_flag("unlocked", "true");

    // Now step should be visible
    assert_eq!(installer.visible_steps().len(), 1);
}

#[test]
fn resolve_includes_files_from_hidden_steps() {
    // This is the documented behavior: visibility is a UI concern, resolver includes all selections
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps>
            <installStep name="Always">
                <optionalFileGroups><group name="G" type="SelectAny">
                <plugins><plugin name="P1"><typeDescriptor><type name="Optional"/></typeDescriptor>
                    <conditionFlags><flag name="show">no</flag></conditionFlags>
                    <files><file source="always.esp"/></files>
                </plugin></plugins>
                </group></optionalFileGroups>
            </installStep>
            <installStep name="Hidden">
                <visible><flagDependency flag="show" value="yes"/></visible>
                <optionalFileGroups><group name="G" type="SelectAny">
                <plugins><plugin name="P2"><typeDescriptor><type name="Optional"/></typeDescriptor>
                    <files><file source="hidden.esp"/></files>
                </plugin></plugins>
                </group></optionalFileGroups>
            </installStep>
        </installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select from always-visible step (sets show=no)
    installer.select(0, 0, vec![0]);
    // Force-select from hidden step too
    installer.select(1, 0, vec![0]);

    // Hidden step should indeed be hidden
    assert_eq!(installer.visible_steps().len(), 1);

    // But files from hidden step are still in the plan
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "always.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "hidden.esp"));
}

#[test]
fn plugin_with_flags_but_no_files() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectExactlyOne">
        <plugins>
            <plugin name="FlagOnly">
                <conditionFlags><flag name="choice">flag_only</flag></conditionFlags>
                <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
            <plugin name="WithFiles">
                <conditionFlags><flag name="choice">with_files</flag></conditionFlags>
                <typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="mod.esp"/></files>
            </plugin>
        </plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select the flag-only plugin
    installer.select(0, 0, vec![0]);
    assert_eq!(
        installer.context().flags.get("choice"),
        Some(&"flag_only".to_string())
    );

    // Plan should be empty (no files from flag-only plugin)
    let plan = installer.resolve();
    assert!(plan.operations.is_empty());
}

#[test]
fn multiple_conditional_patterns_all_matching() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <conditionFlags>
                <flag name="a">yes</flag>
                <flag name="b">yes</flag>
            </conditionFlags>
            <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps>
        <conditionalFileInstalls><patterns>
            <pattern>
                <dependencies><flagDependency flag="a" value="yes"/></dependencies>
                <files><file source="from_a.esp"/></files>
            </pattern>
            <pattern>
                <dependencies><flagDependency flag="b" value="yes"/></dependencies>
                <files><file source="from_b.esp"/></files>
            </pattern>
            <pattern>
                <dependencies operator="And">
                    <flagDependency flag="a" value="yes"/>
                    <flagDependency flag="b" value="yes"/>
                </dependencies>
                <files><file source="from_both.esp"/></files>
            </pattern>
        </patterns></conditionalFileInstalls></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![0]);

    let plan = installer.resolve();
    // All three conditional patterns should match
    assert!(plan.operations.iter().any(|op| op.source == "from_a.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "from_b.esp"));
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "from_both.esp")
    );
}

#[test]
fn priority_sort_negative_zero_positive() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <requiredInstallFiles>
            <file source="zero.esp" priority="0"/>
            <file source="neg.esp" priority="-5"/>
            <file source="pos.esp" priority="5"/>
            <file source="neg2.esp" priority="-10"/>
            <file source="pos2.esp" priority="10"/>
        </requiredInstallFiles></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let plan = installer.resolve();

    let priorities: Vec<i32> = plan.operations.iter().map(|op| op.priority).collect();
    assert_eq!(priorities, vec![-10, -5, 0, 5, 10]);
}

#[test]
fn selecting_empty_list_clears_previous() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins>
            <plugin name="A">
                <conditionFlags><flag name="sel">a</flag></conditionFlags>
                <typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="a.esp"/></files>
            </plugin>
        </plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select A
    installer.select(0, 0, vec![0]);
    assert!(installer.resolve().operations.iter().any(|op| op.source == "a.esp"));

    // Deselect all (SelectAny allows empty)
    installer.select(0, 0, vec![]);
    assert!(!installer.resolve().operations.iter().any(|op| op.source == "a.esp"));
    // Flag should be cleared
    assert!(!installer.context().flags.contains_key("sel"));
}

#[test]
fn validate_selection_empty_group() {
    // A group with 0 plugins
    let group = fomod_oxide::config::Group {
        name: "Empty".into(),
        group_type: GroupType::SelectAny,
        plugins: fomod_oxide::config::PluginList {
            order: None,
            plugins: vec![],
        },
    };
    // Empty selection on empty group should be fine for SelectAny
    assert!(Installer::validate_selection(&group, &[]).is_ok());

    // Any index is out of bounds
    assert!(Installer::validate_selection(&group, &[0]).is_err());
}

#[test]
fn validate_select_all_empty_group() {
    let group = fomod_oxide::config::Group {
        name: "Empty".into(),
        group_type: GroupType::SelectAll,
        plugins: fomod_oxide::config::PluginList {
            order: None,
            plugins: vec![],
        },
    };
    // SelectAll with 0 plugins: selecting 0 == all 0 → ok
    assert!(Installer::validate_selection(&group, &[]).is_ok());
}

#[test]
fn default_selections_could_be_usable_not_selected() {
    // CouldBeUsable should NOT be selected by default
    let group = fomod_oxide::config::Group {
        name: "G".into(),
        group_type: GroupType::SelectAny,
        plugins: fomod_oxide::config::PluginList {
            order: None,
            plugins: vec![
                fomod_oxide::config::Plugin {
                    name: "P1".into(),
                    description: None,
                    image: None,
                    type_descriptor: Some(fomod_oxide::config::TypeDescriptor {
                        simple_type: Some(fomod_oxide::config::SimpleType {
                            name: PluginType::CouldBeUsable,
                        }),
                        dependency_type: None,
                    }),
                    condition_flags: None,
                    files: None,
                },
                fomod_oxide::config::Plugin {
                    name: "P2".into(),
                    description: None,
                    image: None,
                    type_descriptor: Some(fomod_oxide::config::TypeDescriptor {
                        simple_type: Some(fomod_oxide::config::SimpleType {
                            name: PluginType::NotUsable,
                        }),
                        dependency_type: None,
                    }),
                    condition_flags: None,
                    files: None,
                },
            ],
        },
    };
    assert!(Installer::default_selections(&group).is_empty());
}

#[test]
fn default_selections_at_least_one_picks_all_required_recommended() {
    let group = fomod_oxide::config::Group {
        name: "G".into(),
        group_type: GroupType::SelectAtLeastOne,
        plugins: fomod_oxide::config::PluginList {
            order: None,
            plugins: vec![
                fomod_oxide::config::Plugin {
                    name: "Opt".into(),
                    description: None,
                    image: None,
                    type_descriptor: Some(fomod_oxide::config::TypeDescriptor {
                        simple_type: Some(fomod_oxide::config::SimpleType {
                            name: PluginType::Optional,
                        }),
                        dependency_type: None,
                    }),
                    condition_flags: None,
                    files: None,
                },
                fomod_oxide::config::Plugin {
                    name: "Req".into(),
                    description: None,
                    image: None,
                    type_descriptor: Some(fomod_oxide::config::TypeDescriptor {
                        simple_type: Some(fomod_oxide::config::SimpleType {
                            name: PluginType::Required,
                        }),
                        dependency_type: None,
                    }),
                    condition_flags: None,
                    files: None,
                },
                fomod_oxide::config::Plugin {
                    name: "Rec".into(),
                    description: None,
                    image: None,
                    type_descriptor: Some(fomod_oxide::config::TypeDescriptor {
                        simple_type: Some(fomod_oxide::config::SimpleType {
                            name: PluginType::Recommended,
                        }),
                        dependency_type: None,
                    }),
                    condition_flags: None,
                    files: None,
                },
            ],
        },
    };
    // AtLeastOne picks all Required and Recommended
    assert_eq!(Installer::default_selections(&group), vec![1, 2]);
}

// =====================================================================
// Declarative additional edge cases
// =====================================================================

#[test]
fn declarative_schema_version_zero_allowed() {
    // Past versions (0) should be allowed since they're <= current
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    let decl = DeclarativeConfig {
        schema_version: 0,
        rev: "old".into(),
        hash: hash_xml(xml),
        selections: HashMap::new(),
    };
    assert!(decl.apply(xml, &mut installer).is_ok());
}

#[test]
fn declarative_apply_invalid_group_name() {
    let xml = include_str!("fixtures/simple_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "test".into(),
        hash: hash_xml(xml),
        selections: HashMap::from([(
            "Choose Textures".into(),
            HashMap::from([("Nonexistent Group".into(), vec![])]),
        )]),
    };
    let err = decl.apply(xml, &mut installer).unwrap_err();
    assert!(err.to_string().contains("Nonexistent Group"));
}

#[test]
fn declarative_apply_validation_fails_exactly_one_with_none() {
    let xml = include_str!("fixtures/simple_config.xml");
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "test".into(),
        hash: hash_xml(xml),
        selections: HashMap::from([(
            "Choose Textures".into(),
            HashMap::from([("Texture Quality".into(), vec![])]),
        )]),
    };
    let err = decl.apply(xml, &mut installer).unwrap_err();
    assert!(err.to_string().contains("invalid selection"));
}

#[test]
fn declarative_from_defaults_no_steps() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let decl = DeclarativeConfig::from_defaults(xml, "1.0", &config);
    assert!(decl.selections.is_empty());
}

#[test]
fn declarative_from_all_no_steps() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let decl = DeclarativeConfig::from_all(xml, "1.0", &config);
    assert!(decl.selections.is_empty());
}

// =====================================================================
// SelectionError additional tests
// =====================================================================

#[test]
fn selection_error_display_messages() {
    let e1 = fomod_oxide::SelectionError::OutOfBounds;
    assert_eq!(e1.to_string(), "plugin index out of bounds");

    let e2 = fomod_oxide::SelectionError::InvalidCount {
        expected: "exactly 1",
        got: 0,
    };
    assert!(e2.to_string().contains("exactly 1"));
    assert!(e2.to_string().contains("0"));
}

#[test]
fn selection_error_is_std_error() {
    let e: Box<dyn std::error::Error> =
        Box::new(fomod_oxide::SelectionError::OutOfBounds);
    assert!(!e.to_string().is_empty());
}

#[test]
fn fomod_error_from_xml_parse() {
    let result = ModuleConfig::parse("not xml");
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("XML"));
}

// =====================================================================
// FomodInfo additional edge cases
// =====================================================================

#[test]
fn info_xml_all_empty_fields() {
    let xml = r#"
        <fomod>
            <Name></Name>
            <Author></Author>
            <Version></Version>
            <Description></Description>
            <Website></Website>
            <Id></Id>
        </fomod>
    "#;
    let info = FomodInfo::parse(xml).unwrap();
    // Empty string fields should parse as Some("")
    assert!(info.name.is_some());
    assert!(info.author.is_some());
}

#[test]
fn info_xml_special_characters() {
    let xml = r#"
        <fomod>
            <Name>Mod &amp; Expansion &lt;v2&gt;</Name>
            <Description>Contains "quotes" and 'apostrophes'</Description>
        </fomod>
    "#;
    let info = FomodInfo::parse(xml).unwrap();
    assert_eq!(info.name.as_deref(), Some("Mod & Expansion <v2>"));
}

// =====================================================================
// Hash edge cases
// =====================================================================

#[test]
fn hash_xml_unicode_content() {
    let h1 = hash_xml("日本語");
    let h2 = hash_xml("日本語");
    assert_eq!(h1, h2);
    assert_ne!(hash_xml("日本語"), hash_xml("中文"));
}

#[test]
fn hash_xml_very_long_content() {
    let big = "x".repeat(1_000_000);
    let h = hash_xml(&big);
    assert!(h.starts_with("sha256-"));
    // Should be deterministic
    assert_eq!(h, hash_xml(&big));
}

// =====================================================================
// completion_status() tests
// =====================================================================

/// Helper: XML with two steps, each with one SelectExactlyOne group and two plugins.
const TWO_STEP_XML: &str = r#"
<config><moduleName>T</moduleName>
<installSteps order="Explicit">
  <installStep name="Step1">
    <optionalFileGroups><group name="G1" type="SelectExactlyOne">
      <plugins>
        <plugin name="P1"><typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="a.esp" destination="a.esp"/></files>
        </plugin>
        <plugin name="P2"><typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="b.esp" destination="b.esp"/></files>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
  </installStep>
  <installStep name="Step2">
    <optionalFileGroups><group name="G2" type="SelectExactlyOne">
      <plugins>
        <plugin name="P3"><typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="c.esp" destination="c.esp"/></files>
        </plugin>
        <plugin name="P4"><typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="d.esp" destination="d.esp"/></files>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
  </installStep>
</installSteps></config>
"#;

#[test]
fn completion_status_no_steps() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let status = installer.completion_status();
    assert_eq!(status.total_steps, 0);
    assert_eq!(status.visible_steps, 0);
    assert_eq!(status.total_groups, 0);
    assert_eq!(status.satisfied_groups, 0);
    // fraction() with zero groups should be 1.0
    assert!((status.fraction() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn completion_status_partially_satisfied() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    // Only satisfy the first group
    installer.select(0, 0, vec![0]);
    let status = installer.completion_status();
    assert_eq!(status.total_steps, 2);
    assert_eq!(status.visible_steps, 2);
    assert_eq!(status.total_groups, 2);
    assert_eq!(status.satisfied_groups, 1);
    assert!((status.fraction() - 0.5).abs() < f32::EPSILON);
}

#[test]
fn completion_status_fully_satisfied() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.select(1, 0, vec![1]);
    let status = installer.completion_status();
    assert_eq!(status.total_groups, 2);
    assert_eq!(status.satisfied_groups, 2);
    assert!((status.fraction() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn completion_status_select_any_always_satisfied() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps order="Explicit"><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    // SelectAny with zero selections should still be satisfied
    let status = installer.completion_status();
    assert_eq!(status.satisfied_groups, 1);
    assert!((status.fraction() - 1.0).abs() < f32::EPSILON);
}

// =====================================================================
// is_ready_to_install() tests
// =====================================================================

#[test]
fn is_ready_no_groups_returns_false() {
    let xml = r#"<config><moduleName>T</moduleName></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    // total_groups == 0 so not ready
    assert!(!installer.is_ready_to_install());
}

#[test]
fn is_ready_all_satisfied() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.select(1, 0, vec![1]);
    assert!(installer.is_ready_to_install());
}

#[test]
fn is_ready_partially_satisfied() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    // Step2/G2 still has no selection
    assert!(!installer.is_ready_to_install());
}

// =====================================================================
// missing_selections() tests
// =====================================================================

#[test]
fn missing_selections_none_missing() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.select(1, 0, vec![1]);
    assert!(installer.missing_selections().is_empty());
}

#[test]
fn missing_selections_some_missing() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    // Step2 group is missing
    let missing = installer.missing_selections();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], (1, 0));
}

#[test]
fn missing_selections_all_missing() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    let missing = installer.missing_selections();
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&(0, 0)));
    assert!(missing.contains(&(1, 0)));
}

// =====================================================================
// validate_step() tests
// =====================================================================

#[test]
fn validate_step_need_exactly_one() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    // No selection for step 0 which requires SelectExactlyOne
    let hints = installer.validate_step(0);
    assert_eq!(hints.len(), 1);
    match &hints[0] {
        ValidationHint::NeedExactly { group, required, current } => {
            assert_eq!(group, "G1");
            assert_eq!(*required, 1);
            assert_eq!(*current, 0);
        }
        other => panic!("Expected NeedExactly, got {:?}", other),
    }
}

#[test]
fn validate_step_need_at_least_one() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAtLeastOne">
      <plugins>
        <plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="B"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let hints = installer.validate_step(0);
    assert_eq!(hints.len(), 1);
    match &hints[0] {
        ValidationHint::NeedAtLeast { group, required, current } => {
            assert_eq!(group, "G");
            assert_eq!(*required, 1);
            assert_eq!(*current, 0);
        }
        other => panic!("Expected NeedAtLeast, got {:?}", other),
    }
}

#[test]
fn validate_step_exceeds_max() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAtMostOne">
      <plugins>
        <plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="B"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    // Select both plugins when at-most-one is allowed
    installer.select(0, 0, vec![0, 1]);
    let hints = installer.validate_step(0);
    assert_eq!(hints.len(), 1);
    match &hints[0] {
        ValidationHint::ExceedsMax { group, max, current } => {
            assert_eq!(group, "G");
            assert_eq!(*max, 1);
            assert_eq!(*current, 2);
        }
        other => panic!("Expected ExceedsMax, got {:?}", other),
    }
}

#[test]
fn validate_step_not_usable_selected() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="Bad"><typeDescriptor><type name="NotUsable"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    let hints = installer.validate_step(0);
    assert_eq!(hints.len(), 1);
    match &hints[0] {
        ValidationHint::NotUsableSelected { group, plugin } => {
            assert_eq!(group, "G");
            assert_eq!(plugin, "Bad");
        }
        other => panic!("Expected NotUsableSelected, got {:?}", other),
    }
}

#[test]
fn validate_step_nonexistent_step_returns_empty() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    assert!(installer.validate_step(99).is_empty());
}

#[test]
fn validate_step_select_all_incomplete() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAll">
      <plugins>
        <plugin name="A"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="B"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="C"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    let hints = installer.validate_step(0);
    assert_eq!(hints.len(), 1);
    match &hints[0] {
        ValidationHint::NeedExactly { group, required, current } => {
            assert_eq!(group, "G");
            assert_eq!(*required, 3);
            assert_eq!(*current, 1);
        }
        other => panic!("Expected NeedExactly, got {:?}", other),
    }
}

// =====================================================================
// detect_conflicts() tests
// =====================================================================

#[test]
fn detect_conflicts_no_conflicts() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.select(1, 0, vec![0]);
    assert!(installer.detect_conflicts().is_empty());
}

#[test]
fn detect_conflicts_plugins_same_destination() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="A">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="texA.dds" destination="textures/tex.dds"/></files>
        </plugin>
        <plugin name="B">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="texB.dds" destination="textures/tex.dds"/></files>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0, 1]);
    let conflicts = installer.detect_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].destination, "textures/tex.dds");
    assert_eq!(conflicts[0].sources.len(), 2);
}

#[test]
fn detect_conflicts_required_vs_plugin() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <requiredInstallFiles>
      <file source="core.esp" destination="data/shared.esp"/>
    </requiredInstallFiles>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="A">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="extra.esp" destination="data/shared.esp"/></files>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    let conflicts = installer.detect_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].destination, "data/shared.esp");
    // Should have both a Required and a Plugin source
    let has_required = conflicts[0].sources.iter().any(|s| matches!(s, FileConflictSource::Required { .. }));
    let has_plugin = conflicts[0].sources.iter().any(|s| matches!(s, FileConflictSource::Plugin { .. }));
    assert!(has_required);
    assert!(has_plugin);
}

// =====================================================================
// flag_impact_map() tests
// =====================================================================

#[test]
fn flag_impact_map_no_impact() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    assert!(installer.flag_impact_map().is_empty());
}

#[test]
fn flag_impact_map_single_impact() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps order="Explicit">
      <installStep name="Step1">
        <optionalFileGroups><group name="G1" type="SelectExactlyOne">
          <plugins>
            <plugin name="EnableStep2">
              <typeDescriptor><type name="Optional"/></typeDescriptor>
              <conditionFlags><flag name="show_s2">on</flag></conditionFlags>
            </plugin>
          </plugins>
        </group></optionalFileGroups>
      </installStep>
      <installStep name="Step2">
        <visible>
          <flagDependency flag="show_s2" value="on"/>
        </visible>
        <optionalFileGroups><group name="G2" type="SelectAny">
          <plugins>
            <plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
          </plugins>
        </group></optionalFileGroups>
      </installStep>
    </installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let impacts = installer.flag_impact_map();
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0].source_step, 0);
    assert_eq!(impacts[0].source_group, 0);
    assert_eq!(impacts[0].source_plugin, 0);
    assert_eq!(impacts[0].flag_name, "show_s2");
    assert_eq!(impacts[0].affected_step, 1);
    assert_eq!(impacts[0].affected_step_name, "Step2");
}

#[test]
fn flag_impact_map_multi_impact() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps order="Explicit">
      <installStep name="Step1">
        <optionalFileGroups><group name="G1" type="SelectAny">
          <plugins>
            <plugin name="SetA">
              <typeDescriptor><type name="Optional"/></typeDescriptor>
              <conditionFlags><flag name="flagA">on</flag></conditionFlags>
            </plugin>
            <plugin name="SetB">
              <typeDescriptor><type name="Optional"/></typeDescriptor>
              <conditionFlags><flag name="flagB">on</flag></conditionFlags>
            </plugin>
          </plugins>
        </group></optionalFileGroups>
      </installStep>
      <installStep name="Step2">
        <visible><flagDependency flag="flagA" value="on"/></visible>
        <optionalFileGroups><group name="G2" type="SelectAny">
          <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
      </installStep>
      <installStep name="Step3">
        <visible><flagDependency flag="flagB" value="on"/></visible>
        <optionalFileGroups><group name="G3" type="SelectAny">
          <plugins><plugin name="Q"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
      </installStep>
    </installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let impacts = installer.flag_impact_map();
    // SetA -> Step2, SetB -> Step3
    assert_eq!(impacts.len(), 2);
    assert!(impacts.iter().any(|i| i.flag_name == "flagA" && i.affected_step_name == "Step2"));
    assert!(impacts.iter().any(|i| i.flag_name == "flagB" && i.affected_step_name == "Step3"));
}

// =====================================================================
// checkpoint() / rollback() / history_len() tests
// =====================================================================

#[test]
fn checkpoint_and_rollback_basic() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.checkpoint();
    assert_eq!(installer.history_len(), 1);

    // Change selection
    installer.select(0, 0, vec![1]);
    assert_eq!(installer.selections().get(&(0, 0)).unwrap(), &vec![1]);

    // Rollback should restore
    assert!(installer.rollback());
    assert_eq!(installer.selections().get(&(0, 0)).unwrap(), &vec![0]);
    assert_eq!(installer.history_len(), 0);
}

#[test]
fn rollback_empty_history_returns_false() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    assert!(!installer.rollback());
}

#[test]
fn multiple_checkpoints() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);
    installer.checkpoint();

    installer.select(0, 0, vec![1]);
    installer.checkpoint();

    installer.select(0, 0, vec![0, 1]);
    assert_eq!(installer.history_len(), 2);

    // First rollback: back to vec![1]
    assert!(installer.rollback());
    assert_eq!(installer.selections().get(&(0, 0)).unwrap(), &vec![1]);
    assert_eq!(installer.history_len(), 1);

    // Second rollback: back to vec![0]
    assert!(installer.rollback());
    assert_eq!(installer.selections().get(&(0, 0)).unwrap(), &vec![0]);
    assert_eq!(installer.history_len(), 0);

    // Third rollback: nothing left
    assert!(!installer.rollback());
}

#[test]
fn checkpoint_preserves_flags() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps order="Explicit">
      <installStep name="S1">
        <optionalFileGroups><group name="G" type="SelectExactlyOne">
          <plugins>
            <plugin name="On">
              <typeDescriptor><type name="Optional"/></typeDescriptor>
              <conditionFlags><flag name="myflag">yes</flag></conditionFlags>
            </plugin>
            <plugin name="Off">
              <typeDescriptor><type name="Optional"/></typeDescriptor>
              <conditionFlags><flag name="myflag">no</flag></conditionFlags>
            </plugin>
          </plugins>
        </group></optionalFileGroups>
      </installStep>
      <installStep name="S2">
        <visible><flagDependency flag="myflag" value="yes"/></visible>
        <optionalFileGroups><group name="G2" type="SelectAny">
          <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
        </group></optionalFileGroups>
      </installStep>
    </installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);

    // Select "On" which sets myflag=yes -> Step2 visible
    installer.select(0, 0, vec![0]);
    let visible_before = installer.visible_steps().len();
    assert_eq!(visible_before, 2);
    installer.checkpoint();

    // Select "Off" which sets myflag=no -> Step2 hidden
    installer.select(0, 0, vec![1]);
    assert_eq!(installer.visible_steps().len(), 1);

    // Rollback should restore flag state too
    assert!(installer.rollback());
    assert_eq!(installer.visible_steps().len(), 2);
}

// =====================================================================
// preview_plugin() tests
// =====================================================================

#[test]
fn preview_plugin_with_files() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    let ops = installer.preview_plugin(0, 0, 0);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].source, "a.esp");
    assert_eq!(ops[0].destination, "a.esp");
}

#[test]
fn preview_plugin_without_files() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="NoFiles"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    let ops = installer.preview_plugin(0, 0, 0);
    assert!(ops.is_empty());
}

#[test]
fn preview_plugin_nonexistent() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    assert!(installer.preview_plugin(99, 0, 0).is_empty());
    assert!(installer.preview_plugin(0, 99, 0).is_empty());
    assert!(installer.preview_plugin(0, 0, 99).is_empty());
}

// =====================================================================
// preview_current() vs resolve() tests
// =====================================================================

#[test]
fn preview_current_excludes_conditionals() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <requiredInstallFiles>
      <file source="base.esp" destination="base.esp"/>
    </requiredInstallFiles>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectExactlyOne">
      <plugins>
        <plugin name="A">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
          <files><file source="a.esp" destination="a.esp"/></files>
          <conditionFlags><flag name="chose_a">true</flag></conditionFlags>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps>
    <conditionalFileInstalls>
      <patterns>
        <pattern>
          <dependencies><flagDependency flag="chose_a" value="true"/></dependencies>
          <files><file source="bonus.esp" destination="bonus.esp"/></files>
        </pattern>
      </patterns>
    </conditionalFileInstalls>
    </config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let mut installer = Installer::new(config);
    installer.select(0, 0, vec![0]);

    let preview = installer.preview_current();
    let resolved = installer.resolve();

    // preview should not include the conditional bonus.esp
    let preview_has_bonus = preview.operations.iter().any(|op| op.source == "bonus.esp");
    assert!(!preview_has_bonus, "preview_current should exclude conditional files");

    // resolve should include it
    let resolve_has_bonus = resolved.operations.iter().any(|op| op.source == "bonus.esp");
    assert!(resolve_has_bonus, "resolve should include conditional files");

    // Both should have base.esp and a.esp
    assert!(preview.operations.iter().any(|op| op.source == "base.esp"));
    assert!(preview.operations.iter().any(|op| op.source == "a.esp"));
    assert!(resolved.operations.iter().any(|op| op.source == "base.esp"));
    assert!(resolved.operations.iter().any(|op| op.source == "a.esp"));
}

// =====================================================================
// Metadata accessors tests
// =====================================================================

#[test]
fn step_name_valid_and_invalid() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.step_name(0), Some("Step1"));
    assert_eq!(installer.step_name(1), Some("Step2"));
    assert_eq!(installer.step_name(99), None);
}

#[test]
fn group_name_valid_and_invalid() {
    let config = ModuleConfig::parse(TWO_STEP_XML).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.group_name(0, 0), Some("G1"));
    assert_eq!(installer.group_name(1, 0), Some("G2"));
    assert_eq!(installer.group_name(99, 0), None);
    assert_eq!(installer.group_name(0, 99), None);
}

#[test]
fn plugin_description_present_and_absent() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="WithDesc">
          <description>A cool plugin</description>
          <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin>
        <plugin name="NoDesc">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.plugin_description(0, 0, 0), Some("A cool plugin"));
    assert_eq!(installer.plugin_description(0, 0, 1), None);
    assert_eq!(installer.plugin_description(99, 0, 0), None);
}

#[test]
fn plugin_image_path_present_and_absent() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="WithImg">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
          <image path="images/preview.png"/>
        </plugin>
        <plugin name="NoImg">
          <typeDescriptor><type name="Optional"/></typeDescriptor>
        </plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.plugin_image_path(0, 0, 0), Some("images/preview.png"));
    assert_eq!(installer.plugin_image_path(0, 0, 1), None);
    assert_eq!(installer.plugin_image_path(0, 0, 99), None);
}

#[test]
fn module_image_path_present_and_absent() {
    let xml_with = r#"
    <config><moduleName>T</moduleName>
    <moduleImage path="banner.png"/>
    </config>"#;
    let xml_without = r#"<config><moduleName>T</moduleName></config>"#;

    let config_with = ModuleConfig::parse(xml_with).unwrap();
    let installer_with = Installer::new(config_with);
    assert_eq!(installer_with.module_image_path(), Some("banner.png"));

    let config_without = ModuleConfig::parse(xml_without).unwrap();
    let installer_without = Installer::new(config_without);
    assert_eq!(installer_without.module_image_path(), None);
}

#[test]
fn plugin_type_at_valid() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups><group name="G" type="SelectAny">
      <plugins>
        <plugin name="Req"><typeDescriptor><type name="Required"/></typeDescriptor></plugin>
        <plugin name="Rec"><typeDescriptor><type name="Recommended"/></typeDescriptor></plugin>
        <plugin name="Opt"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
        <plugin name="NU"><typeDescriptor><type name="NotUsable"/></typeDescriptor></plugin>
      </plugins>
    </group></optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.plugin_type_at(0, 0, 0), Some(PluginType::Required));
    assert_eq!(installer.plugin_type_at(0, 0, 1), Some(PluginType::Recommended));
    assert_eq!(installer.plugin_type_at(0, 0, 2), Some(PluginType::Optional));
    assert_eq!(installer.plugin_type_at(0, 0, 3), Some(PluginType::NotUsable));
    assert_eq!(installer.plugin_type_at(0, 0, 99), None);
}

#[test]
fn group_type_at_valid_and_invalid() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps><installStep name="S">
    <optionalFileGroups>
      <group name="G1" type="SelectExactlyOne">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
      </group>
      <group name="G2" type="SelectAtMostOne">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
      </group>
      <group name="G3" type="SelectAtLeastOne">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
      </group>
      <group name="G4" type="SelectAll">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
      </group>
      <group name="G5" type="SelectAny">
        <plugins><plugin name="P"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins>
      </group>
    </optionalFileGroups>
    </installStep></installSteps></config>"#;
    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);
    assert_eq!(installer.group_type_at(0, 0), Some(GroupType::SelectExactlyOne));
    assert_eq!(installer.group_type_at(0, 1), Some(GroupType::SelectAtMostOne));
    assert_eq!(installer.group_type_at(0, 2), Some(GroupType::SelectAtLeastOne));
    assert_eq!(installer.group_type_at(0, 3), Some(GroupType::SelectAll));
    assert_eq!(installer.group_type_at(0, 4), Some(GroupType::SelectAny));
    assert_eq!(installer.group_type_at(0, 99), None);
    assert_eq!(installer.group_type_at(99, 0), None);
}

// =====================================================================
// ValidationHint Display tests
// =====================================================================

#[test]
fn validation_hint_display_need_exactly() {
    let hint = ValidationHint::NeedExactly {
        group: "Textures".to_string(),
        required: 1,
        current: 0,
    };
    assert_eq!(hint.to_string(), "Textures: need exactly 1, have 0 selected");
}

#[test]
fn validation_hint_display_need_at_least() {
    let hint = ValidationHint::NeedAtLeast {
        group: "Patches".to_string(),
        required: 1,
        current: 0,
    };
    assert_eq!(hint.to_string(), "Patches: need at least 1, have 0 selected");
}

#[test]
fn validation_hint_display_exceeds_max() {
    let hint = ValidationHint::ExceedsMax {
        group: "Options".to_string(),
        max: 1,
        current: 3,
    };
    assert_eq!(hint.to_string(), "Options: at most 1 allowed, have 3 selected");
}

#[test]
fn validation_hint_display_not_usable() {
    let hint = ValidationHint::NotUsableSelected {
        group: "Broken".to_string(),
        plugin: "OldPlugin".to_string(),
    };
    assert_eq!(
        hint.to_string(),
        "Broken: \"OldPlugin\" is marked as not usable"
    );
}

// =====================================================================
// SelectionSummary Display and summary() tests
// =====================================================================

#[test]
fn selection_summary_display() {
    let summary = SelectionSummary {
        step: "Choose Version".to_string(),
        group: "Platform".to_string(),
        plugins: vec!["SSE".to_string(), "AE".to_string()],
    };
    assert_eq!(summary.to_string(), "Choose Version > Platform: [SSE, AE]");
}

#[test]
fn selection_summary_display_empty_plugins() {
    let summary = SelectionSummary {
        step: "Step".to_string(),
        group: "Group".to_string(),
        plugins: vec![],
    };
    assert_eq!(summary.to_string(), "Step > Group: []");
}

#[test]
fn declarative_config_summary() {
    let xml = r#"
    <config><moduleName>T</moduleName>
    <installSteps order="Explicit">
      <installStep name="Step1">
        <optionalFileGroups><group name="G1" type="SelectExactlyOne">
          <plugins>
            <plugin name="Alpha"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
            <plugin name="Beta"><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
          </plugins>
        </group></optionalFileGroups>
      </installStep>
    </installSteps></config>"#;
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: hash_xml(xml),
        rev: "test".to_string(),
        selections: HashMap::from([(
            "Step1".to_string(),
            HashMap::from([("G1".to_string(), vec!["Alpha".to_string()])]),
        )]),
    };
    let summaries = decl.summary();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].step, "Step1");
    assert_eq!(summaries[0].group, "G1");
    assert_eq!(summaries[0].plugins, vec!["Alpha".to_string()]);
}

#[test]
fn declarative_config_summary_sorted() {
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "test".to_string(),
        selections: HashMap::from([
            (
                "Zebra".to_string(),
                HashMap::from([("G".to_string(), vec!["P".to_string()])]),
            ),
            (
                "Alpha".to_string(),
                HashMap::from([("G".to_string(), vec!["Q".to_string()])]),
            ),
        ]),
    };
    let summaries = decl.summary();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].step, "Alpha");
    assert_eq!(summaries[1].step, "Zebra");
}

// =====================================================================
// diff() tests
// =====================================================================

#[test]
fn diff_identical_configs() {
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([(
            "Step1".to_string(),
            HashMap::from([("G1".to_string(), vec!["A".to_string()])]),
        )]),
    };
    let diffs = decl.diff(&decl);
    assert!(diffs.is_empty());
}

#[test]
fn diff_different_plugins() {
    let a = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([(
            "Step1".to_string(),
            HashMap::from([("G1".to_string(), vec!["Alpha".to_string()])]),
        )]),
    };
    let b = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "2.0".to_string(),
        selections: HashMap::from([(
            "Step1".to_string(),
            HashMap::from([("G1".to_string(), vec!["Beta".to_string()])]),
        )]),
    };
    let diffs = a.diff(&b);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].step, "Step1");
    assert_eq!(diffs[0].group, "G1");
    assert_eq!(diffs[0].left, vec!["Alpha".to_string()]);
    assert_eq!(diffs[0].right, vec!["Beta".to_string()]);
}

#[test]
fn diff_extra_group_in_other() {
    let a = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([(
            "Step1".to_string(),
            HashMap::from([("G1".to_string(), vec!["A".to_string()])]),
        )]),
    };
    let b = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([
            (
                "Step1".to_string(),
                HashMap::from([("G1".to_string(), vec!["A".to_string()])]),
            ),
            (
                "Step2".to_string(),
                HashMap::from([("G2".to_string(), vec!["B".to_string()])]),
            ),
        ]),
    };
    let diffs = a.diff(&b);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].step, "Step2");
    assert_eq!(diffs[0].group, "G2");
    assert!(diffs[0].left.is_empty());
    assert_eq!(diffs[0].right, vec!["B".to_string()]);
}

#[test]
fn diff_symmetric() {
    let a = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([(
            "S".to_string(),
            HashMap::from([("G".to_string(), vec!["X".to_string()])]),
        )]),
    };
    let b = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        hash: "sha256-xxx".to_string(),
        rev: "1.0".to_string(),
        selections: HashMap::from([(
            "S".to_string(),
            HashMap::from([("G".to_string(), vec!["Y".to_string()])]),
        )]),
    };
    let ab = a.diff(&b);
    let ba = b.diff(&a);
    assert_eq!(ab.len(), 1);
    assert_eq!(ba.len(), 1);
    // left/right are swapped
    assert_eq!(ab[0].left, ba[0].right);
    assert_eq!(ab[0].right, ba[0].left);
}
