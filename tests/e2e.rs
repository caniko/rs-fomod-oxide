//! End-to-end tests simulating complete mod installation workflows.

use std::collections::HashMap;
use std::fs;

use fomod_oxide::condition::{EvalContext, FileState};
use fomod_oxide::{
    hash_xml, DeclarativeConfig, FlagImpact, Installer, ModuleConfig, SCHEMA_VERSION,
};

const SIMPLE_CONFIG: &str = include_str!("fixtures/simple_config.xml");
const HDTSMP_LIKE: &str = include_str!("fixtures/hdtsmp_like_config.xml");
const VORTEX_TEST: &str = include_str!("fixtures/vortex_test_config.xml");
const MULTI_STEP: &str = include_str!("fixtures/multi_step_visibility.xml");

// =====================================================================
// Complete interactive wizard flows
// =====================================================================

#[test]
fn e2e_simple_high_res_flow() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // Step 0: Choose Textures (always visible)
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].1.name, "Choose Textures");

    // User selects "High Resolution"
    installer.select(0, 0, vec![0]);

    // Step 1 becomes visible
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 2);
    assert_eq!(visible[1].1.name, "Optional Patches");

    // User doesn't select any optional patches (step 1, group 0: SelectAny → empty is valid)

    // Resolve final plan
    let plan = installer.resolve();

    // Required files always present
    assert!(plan.operations.iter().any(|op| op.source == "readme.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "core_files"));

    // High-res textures selected
    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));
    assert!(!plan.operations.iter().any(|op| op.source == "textures_2k"));

    // Conditional HD patches triggered
    assert!(plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn e2e_simple_standard_res_flow() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // Select standard textures
    installer.select(0, 0, vec![1]);

    // Step 1 stays hidden (texture_quality=standard, not high)
    let visible = installer.visible_steps();
    assert_eq!(visible.len(), 1);

    let plan = installer.resolve();

    // Standard textures
    assert!(plan.operations.iter().any(|op| op.source == "textures_2k"));
    assert!(!plan.operations.iter().any(|op| op.source == "textures_4k"));

    // No conditional HD patches
    assert!(!plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn e2e_simple_with_optional_patches() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // High res
    installer.select(0, 0, vec![0]);

    // Select LOD Optimization patch
    installer.select(1, 0, vec![0]);

    let plan = installer.resolve();

    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));
    assert!(plan.operations.iter().any(|op| op.source == "patches/lod_opt.esp"));
    assert!(plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn e2e_hdtsmp_se_cuda_complete() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Step 0: Introduction (SelectAll, no user action needed)
    // Step 1: Options
    installer.select(1, 0, vec![0]); // SE
    installer.select(1, 1, vec![0]); // CUDA

    // Step 2 (Thanks) should now be visible (SSE=On)
    let visible = installer.visible_steps();
    let names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(names.contains(&"Thanks"));

    let plan = installer.resolve();

    // Required files
    assert!(plan.operations.iter().any(|op| op.source == "Licence.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "Readme.txt"));

    // Correct conditional: SE + CUDA
    assert!(plan.operations.iter().any(|op| op.source == "SE_CUDA\\SKSE"));

    // No other conditionals
    let skse_ops: Vec<&str> = plan
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();
    assert_eq!(skse_ops, vec!["SE_CUDA\\SKSE"]);
}

#[test]
fn e2e_hdtsmp_ae_no_cuda() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    installer.select(1, 0, vec![1]); // AE
    installer.select(1, 1, vec![1]); // No CUDA

    let plan = installer.resolve();

    let skse_ops: Vec<&str> = plan
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();
    assert_eq!(skse_ops, vec!["AE_NOCUDA\\SKSE"]);
}

#[test]
fn e2e_hdtsmp_vr_no_conditionals() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    installer.select(1, 0, vec![2]); // VR (SSE=Off, AE=Off)
    installer.select(1, 1, vec![1]); // No CUDA

    let plan = installer.resolve();

    // No SKSE conditionals should trigger (VR doesn't match any)
    let skse_ops: Vec<&str> = plan
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();
    assert!(skse_ops.is_empty(), "VR should not trigger any SKSE patterns");

    // Thanks step should be hidden
    let visible = installer.visible_steps();
    let names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(!names.contains(&"Thanks"));
}

#[test]
fn e2e_hdtsmp_switching_platforms() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Start with SE + CUDA
    installer.select(1, 0, vec![0]);
    installer.select(1, 1, vec![0]);

    let plan1 = installer.resolve();
    assert!(plan1.operations.iter().any(|op| op.source == "SE_CUDA\\SKSE"));

    // Switch to AE (keep CUDA)
    installer.select(1, 0, vec![1]);
    let plan2 = installer.resolve();
    assert!(plan2.operations.iter().any(|op| op.source == "AE_CUDA\\SKSE"));
    assert!(!plan2.operations.iter().any(|op| op.source == "SE_CUDA\\SKSE"));
}

// =====================================================================
// Complete declarative flows
// =====================================================================

#[test]
fn e2e_declarative_roundtrip_simple() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();

    // Generate defaults
    let decl = DeclarativeConfig::from_defaults(SIMPLE_CONFIG, "1.2.0", &config);

    // Apply to new installer
    let config2 = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config2);
    decl.apply(SIMPLE_CONFIG, &mut installer).unwrap();

    let plan = installer.resolve();

    // Should produce same result as manual high-res selection (default = Recommended)
    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));
    assert!(plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn e2e_declarative_custom_selection() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.2.0".into(),
        hash: hash_xml(SIMPLE_CONFIG),
        selections: HashMap::from([
            (
                "Choose Textures".into(),
                HashMap::from([(
                    "Texture Quality".into(),
                    vec!["Standard Resolution".into()],
                )]),
            ),
            (
                "Optional Patches".into(),
                HashMap::from([(
                    "Performance Patches".into(),
                    vec!["LOD Optimization".into()],
                )]),
            ),
        ]),
    };

    decl.apply(SIMPLE_CONFIG, &mut installer).unwrap();
    let plan = installer.resolve();

    // Standard textures
    assert!(plan.operations.iter().any(|op| op.source == "textures_2k"));
    assert!(!plan.operations.iter().any(|op| op.source == "textures_4k"));

    // LOD patch selected
    assert!(plan.operations.iter().any(|op| op.source == "patches/lod_opt.esp"));

    // No conditional HD patches (texture_quality=standard)
    assert!(!plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn e2e_declarative_hdtsmp() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(HDTSMP_LIKE),
        selections: HashMap::from([(
            "Options".into(),
            HashMap::from([
                ("Platform".into(), vec!["AE".into()]),
                ("CUDA".into(), vec!["CUDA".into()]),
            ]),
        )]),
    };

    decl.apply(HDTSMP_LIKE, &mut installer).unwrap();
    let plan = installer.resolve();

    assert!(plan.operations.iter().any(|op| op.source == "AE_CUDA\\SKSE"));
    assert!(plan.operations.iter().any(|op| op.source == "Licence.txt"));
}

// =====================================================================
// Context-aware workflows
// =====================================================================

#[test]
fn e2e_with_preset_game_context() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <gameDependency version="1.5.0"/>
            <fileDependency file="SKSE64_loader.exe" state="Active"/>
        </moduleDependencies>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <typeDescriptor><type name="Optional"/></typeDescriptor>
            <files><file source="mod.esp" destination="Data"/></files>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    // Without proper context → deps fail
    let installer = Installer::new(config.clone());
    assert!(!installer.check_dependencies());

    // With proper context → deps pass
    let mut ctx = EvalContext::new();
    ctx.game_version = Some("1.6.0".into());
    ctx.set_file_state("SKSE64_loader.exe", FileState::Active);

    let mut installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());

    installer.select(0, 0, vec![0]);
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "mod.esp"));
}

// =====================================================================
// File system execution (tempdir)
// =====================================================================

#[test]
fn e2e_install_plan_execute() {
    let tmp = std::env::temp_dir().join("fomod_oxide_test_execute");
    let source_dir = tmp.join("source");
    let dest_dir = tmp.join("dest");

    // Clean up from previous runs
    let _ = fs::remove_dir_all(&tmp);

    // Create source structure
    fs::create_dir_all(source_dir.join("textures")).unwrap();
    fs::write(source_dir.join("readme.txt"), "readme content").unwrap();
    fs::write(source_dir.join("textures/diffuse.dds"), "texture data").unwrap();

    // Build a simple plan
    let plan = fomod_oxide::InstallPlan {
        operations: vec![
            fomod_oxide::FileOperation {
                source: "readme.txt".into(),
                destination: "Data/readme.txt".into(),
                is_folder: false,
                priority: 0,
            },
            fomod_oxide::FileOperation {
                source: "textures".into(),
                destination: "Data/textures".into(),
                is_folder: true,
                priority: 1,
            },
        ],
    };

    plan.execute(&source_dir, &dest_dir).unwrap();

    // Verify files were copied
    assert!(dest_dir.join("Data/readme.txt").exists());
    assert_eq!(
        fs::read_to_string(dest_dir.join("Data/readme.txt")).unwrap(),
        "readme content"
    );
    assert!(dest_dir.join("Data/textures/diffuse.dds").exists());

    // Cleanup
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn e2e_install_plan_execute_empty_destination() {
    let tmp = std::env::temp_dir().join("fomod_oxide_test_empty_dest");
    let source_dir = tmp.join("source");
    let dest_dir = tmp.join("dest");

    let _ = fs::remove_dir_all(&tmp);

    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("mod.esp"), "esp data").unwrap();

    let plan = fomod_oxide::InstallPlan {
        operations: vec![fomod_oxide::FileOperation {
            source: "mod.esp".into(),
            destination: "".into(), // empty → use source path
            is_folder: false,
            priority: 0,
        }],
    };

    plan.execute(&source_dir, &dest_dir).unwrap();

    // Should be at dest_dir/mod.esp (using source path)
    assert!(dest_dir.join("mod.esp").exists());

    let _ = fs::remove_dir_all(&tmp);
}

// =====================================================================
// Multi-step advanced flow
// =====================================================================

#[test]
fn e2e_multi_step_expert_full_flow() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let mut installer = Installer::new(config);

    // Initially only "Choose Mode" visible
    assert_eq!(installer.visible_steps().len(), 1);

    // Select Expert
    installer.select(0, 0, vec![2]);

    let visible = installer.visible_steps();
    let names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert_eq!(names, vec!["Choose Mode", "Expert Options", "Summary"]);

    // Select debug mode in expert options
    installer.select(2, 0, vec![0]);

    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "debug.esp"));
}

#[test]
fn e2e_change_mind_mid_wizard() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let mut installer = Installer::new(config);

    // Start with Advanced
    installer.select(0, 0, vec![1]);
    assert_eq!(installer.visible_steps().len(), 3); // Choose + Advanced + Summary

    // Select tweak A
    installer.select(1, 0, vec![0]);

    // Change mind: go back to Basic
    installer.select(0, 0, vec![0]);
    assert_eq!(installer.visible_steps().len(), 1); // Only Choose

    // The tweak selection is still recorded but won't matter since step is hidden
    let plan = installer.resolve();
    // tweak_a.esp should still appear since the selection record persists
    // (visibility is UI concern, not resolver concern)
    assert!(plan.operations.iter().any(|op| op.source == "tweak_a.esp"));
}

// =====================================================================
// Vortex test complete flow
// =====================================================================

#[test]
fn e2e_declarative_multi_step_with_visibility() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let mut installer = Installer::new(config);

    // Declarative config that selects Expert mode and debug option
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(MULTI_STEP),
        selections: HashMap::from([
            (
                "Choose Mode".into(),
                HashMap::from([("Mode".into(), vec!["Expert".into()])]),
            ),
            (
                "Expert Options".into(),
                HashMap::from([(
                    "Expert Tweaks".into(),
                    vec!["Debug Mode".into()],
                )]),
            ),
        ]),
    };

    decl.apply(MULTI_STEP, &mut installer).unwrap();
    let plan = installer.resolve();

    assert!(plan.operations.iter().any(|op| op.source == "debug.esp"));
}

#[test]
fn e2e_declarative_partial_defaults_fill_in() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // Only specify Optional Patches, let Choose Textures use defaults
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(SIMPLE_CONFIG),
        selections: HashMap::from([(
            "Optional Patches".into(),
            HashMap::from([(
                "Performance Patches".into(),
                vec!["LOD Optimization".into()],
            )]),
        )]),
    };

    decl.apply(SIMPLE_CONFIG, &mut installer).unwrap();
    let plan = installer.resolve();

    // Default for Choose Textures is High Resolution (Recommended)
    assert!(plan.operations.iter().any(|op| op.source == "textures_4k"));
    // LOD patch selected
    assert!(plan.operations.iter().any(|op| op.source == "patches/lod_opt.esp"));
    // Conditional for high-res should trigger
    assert!(plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

// =====================================================================
// Dependency type pattern flows
// =====================================================================

const DEP_TYPE_XML: &str = include_str!("fixtures/dependency_type_patterns.xml");

#[test]
fn e2e_dep_type_windows_complete_flow() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Select Windows
    installer.select(0, 0, vec![0]);

    let plan = installer.resolve();

    // Plugin file from selection
    assert!(plan.operations.iter().any(|op| op.source == "win_base.dll"));

    // Conditional: both desktop and windows patterns match
    assert!(plan.operations.iter().any(|op| op.source == "desktop_extras.dll"));
    assert!(plan.operations.iter().any(|op| op.source == "win_extras.dll"));
    assert!(!plan.operations.iter().any(|op| op.source == "deck_extras.dll"));
}

#[test]
fn e2e_dep_type_steamdeck_flow() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    installer.select(0, 0, vec![2]); // SteamDeck

    let plan = installer.resolve();

    assert!(plan.operations.iter().any(|op| op.source == "deck_base.so"));
    assert!(plan.operations.iter().any(|op| op.source == "deck_extras.dll"));
    // Desktop extras should NOT be present (os_family=handheld, not desktop)
    assert!(!plan.operations.iter().any(|op| op.source == "desktop_extras.dll"));
}

#[test]
fn e2e_dep_type_switching_platforms() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    // Start with Windows
    installer.select(0, 0, vec![0]);
    let plan1 = installer.resolve();
    assert!(plan1.operations.iter().any(|op| op.source == "win_extras.dll"));

    // Switch to Linux
    installer.select(0, 0, vec![1]);
    let plan2 = installer.resolve();
    // Linux is desktop but not windows
    assert!(plan2.operations.iter().any(|op| op.source == "desktop_extras.dll"));
    assert!(!plan2.operations.iter().any(|op| op.source == "win_extras.dll"));
}

// =====================================================================
// InstallPlan execution edge cases
// =====================================================================

#[test]
fn e2e_install_plan_execute_nested_folders() {
    let tmp = std::env::temp_dir().join("fomod_oxide_test_nested");
    let source_dir = tmp.join("source");
    let dest_dir = tmp.join("dest");
    let _ = fs::remove_dir_all(&tmp);

    // Create nested folder structure
    fs::create_dir_all(source_dir.join("data/meshes/actors")).unwrap();
    fs::write(source_dir.join("data/meshes/actors/body.nif"), "mesh").unwrap();
    fs::write(source_dir.join("data/meshes/actors/head.nif"), "mesh2").unwrap();
    fs::create_dir_all(source_dir.join("data/textures")).unwrap();
    fs::write(source_dir.join("data/textures/skin.dds"), "tex").unwrap();

    let plan = fomod_oxide::InstallPlan {
        operations: vec![
            fomod_oxide::FileOperation {
                source: "data".into(),
                destination: "GameData".into(),
                is_folder: true,
                priority: 0,
            },
        ],
    };

    plan.execute(&source_dir, &dest_dir).unwrap();

    assert!(dest_dir.join("GameData/meshes/actors/body.nif").exists());
    assert!(dest_dir.join("GameData/meshes/actors/head.nif").exists());
    assert!(dest_dir.join("GameData/textures/skin.dds").exists());

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn e2e_install_plan_priority_overwrite() {
    let tmp = std::env::temp_dir().join("fomod_oxide_test_priority_overwrite");
    let source_dir = tmp.join("source");
    let dest_dir = tmp.join("dest");
    let _ = fs::remove_dir_all(&tmp);

    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("base.cfg"), "original").unwrap();
    fs::write(source_dir.join("override.cfg"), "patched").unwrap();

    // Lower priority file first, then higher priority overwrites same destination
    let plan = fomod_oxide::InstallPlan {
        operations: vec![
            fomod_oxide::FileOperation {
                source: "base.cfg".into(),
                destination: "config.cfg".into(),
                is_folder: false,
                priority: 0,
            },
            fomod_oxide::FileOperation {
                source: "override.cfg".into(),
                destination: "config.cfg".into(),
                is_folder: false,
                priority: 10,
            },
        ],
    };

    plan.execute(&source_dir, &dest_dir).unwrap();

    // Higher priority file should overwrite
    let content = fs::read_to_string(dest_dir.join("config.cfg")).unwrap();
    assert_eq!(content, "patched");

    let _ = fs::remove_dir_all(&tmp);
}

// =====================================================================
// Multi-step visibility with flags from multiple groups
// =====================================================================

#[test]
fn e2e_multi_step_visibility_toggle_back_and_forth() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let mut installer = Installer::new(config);

    // Basic → 1 visible
    installer.select(0, 0, vec![0]);
    assert_eq!(installer.visible_steps().len(), 1);

    // Advanced → 3 visible (Choose + Advanced + Summary)
    installer.select(0, 0, vec![1]);
    assert_eq!(installer.visible_steps().len(), 3);

    // Expert → 3 visible (Choose + Expert + Summary)
    installer.select(0, 0, vec![2]);
    let names: Vec<&str> = installer
        .visible_steps()
        .iter()
        .map(|(_, s)| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["Choose Mode", "Expert Options", "Summary"]);

    // Back to Basic → 1 visible
    installer.select(0, 0, vec![0]);
    assert_eq!(installer.visible_steps().len(), 1);
}

// =====================================================================
// Context-aware with all dependency types combined
// =====================================================================

#[test]
fn e2e_all_dependency_types_combined() {
    let xml = r#"
        <config><moduleName>T</moduleName>
        <moduleDependencies operator="And">
            <flagDependency flag="enabled" value="yes"/>
            <fileDependency file="SKSE64_loader.exe" state="Active"/>
            <gameDependency version="1.5.0"/>
            <fommDependency version="2.0.0"/>
        </moduleDependencies>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins><plugin name="P">
            <typeDescriptor><type name="Optional"/></typeDescriptor>
            <files><file source="mod.esp"/></files>
        </plugin></plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;
    let config = ModuleConfig::parse(xml).unwrap();

    let mut ctx = EvalContext::new();
    ctx.set_flag("enabled", "yes");
    ctx.set_file_state("SKSE64_loader.exe", FileState::Active);
    ctx.game_version = Some("1.6.0".into());
    ctx.manager_version = Some("2.5.0".into());

    let mut installer = Installer::with_context(config, ctx);
    assert!(installer.check_dependencies());

    installer.select(0, 0, vec![0]);
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "mod.esp"));
}

#[test]
fn e2e_vortex_full_selection_and_dependency_types() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Select File A in group 0 (no recommend)
    installer.select(0, 0, vec![0]);
    // Select File A in group 1 (with recommend)
    installer.select(0, 1, vec![0]);

    // Verify flags
    let ctx = installer.context();
    assert_eq!(ctx.flags.get("fileanorec"), Some(&"On".to_string()));
    assert_eq!(ctx.flags.get("filearec"), Some(&"On".to_string()));

    // Check plugin types in context
    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    use fomod_oxide::config::PluginType;

    // First A: fileanorec=On → Required
    assert_eq!(
        results_group.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::Required
    );
    // First B: filebnorec not set → NotUsable
    assert_eq!(
        results_group.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::NotUsable
    );
    // Second A: filearec=On → Required
    assert_eq!(
        results_group.plugins.plugins[2].plugin_type_in_context(installer.context()),
        PluginType::Required
    );
    // Second B: filebrec not set → NotUsable
    assert_eq!(
        results_group.plugins.plugins[3].plugin_type_in_context(installer.context()),
        PluginType::NotUsable
    );
}

// =====================================================================
// Checkpoint / Rollback wizard flow
// =====================================================================

#[test]
fn e2e_checkpoint_rollback_multi_level() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let mut installer = Installer::new(config);

    // Level 0: select Expert mode
    installer.select(0, 0, vec![2]);
    assert_eq!(installer.visible_steps().len(), 3); // Choose + Expert + Summary

    // Checkpoint after Expert selection
    installer.checkpoint();
    assert_eq!(installer.history_len(), 1);

    // Level 1: select Debug Mode in Expert Options (step 2, group 0)
    installer.select(2, 0, vec![0]);
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "debug.esp"));

    // Checkpoint again
    installer.checkpoint();
    assert_eq!(installer.history_len(), 2);

    // Level 2: switch to Basic mode (this changes visibility)
    installer.select(0, 0, vec![0]);
    assert_eq!(installer.visible_steps().len(), 1); // Only Choose Mode

    // Rollback level 2 -> should restore Expert + Debug
    assert!(installer.rollback());
    assert_eq!(installer.history_len(), 1);

    // Verify Expert mode is restored and Debug is still selected
    let visible = installer.visible_steps();
    let names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(names.contains(&"Expert Options"));
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "debug.esp"));

    // Rollback level 1 -> should restore Expert with no Debug selection
    assert!(installer.rollback());
    assert_eq!(installer.history_len(), 0);

    // Expert mode should still be set (we checkpointed after selecting Expert)
    let visible = installer.visible_steps();
    let names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(names.contains(&"Expert Options"));
    // But debug.esp might still appear since the debug selection was made after checkpoint
    // The rollback restores to the snapshot where debug was NOT selected
    let sel = installer.selections().get(&(2, 0));
    assert!(sel.is_none() || sel.unwrap().is_empty());

    // No more history
    assert!(!installer.rollback());
}

// =====================================================================
// File conflict detection in a real workflow
// =====================================================================

#[test]
fn e2e_file_conflict_detection() {
    // Build a config where two plugins install to the same destination
    let xml = r#"
        <config><moduleName>Conflict Test</moduleName>
        <requiredInstallFiles>
            <file source="base.esp" destination="Data/mod.esp"/>
        </requiredInstallFiles>
        <installSteps><installStep name="S">
        <optionalFileGroups><group name="G" type="SelectAny">
        <plugins>
            <plugin name="PluginA">
                <typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="a_override.esp" destination="Data/mod.esp"/></files>
            </plugin>
            <plugin name="PluginB">
                <typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="b_override.esp" destination="Data/mod.esp"/></files>
            </plugin>
            <plugin name="PluginC">
                <typeDescriptor><type name="Optional"/></typeDescriptor>
                <files><file source="unique.esp" destination="Data/unique.esp"/></files>
            </plugin>
        </plugins>
        </group></optionalFileGroups>
        </installStep></installSteps></config>
    "#;

    let config = ModuleConfig::parse(xml).unwrap();
    let installer = Installer::new(config);

    let conflicts = installer.detect_conflicts();

    // "Data/mod.esp" should be a conflict destination (required + PluginA + PluginB = 3 sources)
    let mod_esp_conflict = conflicts
        .iter()
        .find(|c| c.destination == "data/mod.esp")
        .expect("Expected conflict on data/mod.esp");

    assert_eq!(mod_esp_conflict.sources.len(), 3);

    // "Data/unique.esp" should NOT be in conflicts (only one source)
    assert!(
        !conflicts.iter().any(|c| c.destination == "data/unique.esp"),
        "unique.esp should not be conflicted"
    );
}

// =====================================================================
// Flag impact map e2e
// =====================================================================

#[test]
fn e2e_flag_impact_map() {
    let config = ModuleConfig::parse(MULTI_STEP).unwrap();
    let installer = Installer::new(config);

    let impacts = installer.flag_impact_map();

    // The "mode" flag from step 0 plugins affects:
    // - Step 1 (Advanced Options): visible when mode=advanced
    // - Step 2 (Expert Options): visible when mode=expert
    // - Step 3 (Summary): visible when mode=advanced OR mode=expert
    assert!(!impacts.is_empty());

    // Check that plugins from step 0 are flagged as impacting other steps
    let mode_impacts: Vec<&FlagImpact> = impacts
        .iter()
        .filter(|i| i.flag_name == "mode")
        .collect();
    assert!(!mode_impacts.is_empty());

    // All mode flag setters should come from step 0
    for impact in &mode_impacts {
        assert_eq!(impact.source_step, 0);
    }

    // Mode should affect steps 1 (Advanced Options), 2 (Expert Options), and 3 (Summary)
    let affected_steps: Vec<usize> = mode_impacts.iter().map(|i| i.affected_step).collect();
    assert!(affected_steps.contains(&1), "mode should affect Advanced Options");
    assert!(affected_steps.contains(&2), "mode should affect Expert Options");
    assert!(affected_steps.contains(&3), "mode should affect Summary");

    // Also check "show_expert" flag impact on Expert Options step
    let show_expert_impacts: Vec<&FlagImpact> = impacts
        .iter()
        .filter(|i| i.flag_name == "show_expert")
        .collect();
    assert!(!show_expert_impacts.is_empty());
    assert!(show_expert_impacts.iter().any(|i| i.affected_step_name == "Expert Options"));
}

// =====================================================================
// Preview vs Resolve comparison
// =====================================================================

#[test]
fn e2e_preview_vs_resolve_conditional_files() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // Select High Resolution (triggers conditional hd_normals.esp)
    installer.select(0, 0, vec![0]);

    let preview = installer.preview_current();
    let resolved = installer.resolve();

    // preview_current should NOT include conditional files
    assert!(
        !preview
            .operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp"),
        "preview_current should exclude conditional file installs"
    );

    // resolve should include conditional files
    assert!(
        resolved
            .operations
            .iter()
            .any(|op| op.source == "patches/hd_normals.esp"),
        "resolve should include conditional file installs"
    );

    // Both should include required files
    assert!(preview.operations.iter().any(|op| op.source == "readme.txt"));
    assert!(resolved.operations.iter().any(|op| op.source == "readme.txt"));

    // Both should include selected plugin files
    assert!(preview.operations.iter().any(|op| op.source == "textures_4k"));
    assert!(resolved.operations.iter().any(|op| op.source == "textures_4k"));
}

// =====================================================================
// Completion status tracking through wizard
// =====================================================================

#[test]
fn e2e_completion_status_progression() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Initially: Thanks requires SSE=On or AE=On, neither set -> hidden
    // Visible: Introduction, Options -> 2 visible steps
    // Groups: Introduction.Introduction (SelectAll), Options.Platform, Options.CUDA
    let status0 = installer.completion_status();
    assert_eq!(status0.visible_steps, 2);
    assert_eq!(status0.total_groups, 3);
    // SelectAll with empty selection is invalid; two SelectExactlyOne are invalid
    assert!(status0.satisfied_groups < status0.total_groups);

    // Select SE platform -> SSE=On, which makes Thanks step visible
    installer.select(1, 0, vec![0]);
    let status1 = installer.completion_status();
    // Platform is now satisfied, and Thanks step becomes visible (adding Credits group)
    assert!(
        status1.satisfied_groups > status0.satisfied_groups,
        "Selecting Platform should increase satisfied groups"
    );
    // Thanks step is now visible (SSE=On)
    assert_eq!(status1.visible_steps, 3);

    // Select CUDA -> satisfies CUDA group
    installer.select(1, 1, vec![0]);
    let status2 = installer.completion_status();
    // CUDA group is now also satisfied
    assert!(
        status2.satisfied_groups > status1.satisfied_groups,
        "Selecting CUDA should increase satisfied groups"
    );
}

// =====================================================================
// is_ready_to_install progression
// =====================================================================

#[test]
fn e2e_is_ready_to_install_progression() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // Initially not ready (Choose Textures has SelectExactlyOne with no selection)
    assert!(!installer.is_ready_to_install());

    // Select High Resolution
    installer.select(0, 0, vec![0]);

    // Now step 1 (Optional Patches) becomes visible with SelectAny group
    // SelectAny with empty selection is valid, so both groups should be satisfied
    assert!(
        installer.is_ready_to_install(),
        "Should be ready after filling required groups"
    );

    // Switch to Standard (hides Optional Patches step)
    installer.select(0, 0, vec![1]);
    // Only one visible step with one group, already selected
    assert!(installer.is_ready_to_install());
}

// =====================================================================
// missing_selections tracking
// =====================================================================

#[test]
fn e2e_missing_selections_tracking() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    let missing0 = installer.missing_selections();
    // Introduction (SelectAll) needs all plugins selected -> (0,0) is missing
    // Options: Platform (SelectExactlyOne) -> (1,0) is missing
    // Options: CUDA (SelectExactlyOne) -> (1,1) is missing
    assert!(missing0.contains(&(0, 0)), "Introduction group should be missing");
    assert!(missing0.contains(&(1, 0)), "Platform group should be missing");
    assert!(missing0.contains(&(1, 1)), "CUDA group should be missing");

    // Select Platform (SE) -> SSE=On, which also makes Thanks visible (adding (2,0))
    installer.select(1, 0, vec![0]);
    let missing1 = installer.missing_selections();
    assert!(
        !missing1.contains(&(1, 0)),
        "Platform group should no longer be missing"
    );
    // Thanks step is now visible, so (2, 0) might be newly missing
    // But Platform is satisfied, so net change could go either way

    // Select CUDA
    installer.select(1, 1, vec![0]);
    let missing2 = installer.missing_selections();
    assert!(
        !missing2.contains(&(1, 1)),
        "CUDA group should no longer be missing"
    );

    // Select Introduction (required for SelectAll)
    installer.select(0, 0, vec![0]);
    let missing3 = installer.missing_selections();
    assert!(
        !missing3.contains(&(0, 0)),
        "Introduction group should no longer be missing"
    );
    assert!(
        !missing3.contains(&(1, 0)),
        "Platform should still be satisfied"
    );
    assert!(
        !missing3.contains(&(1, 1)),
        "CUDA should still be satisfied"
    );

    // Fill Thanks > Credits (SelectAll with 1 plugin)
    installer.select(2, 0, vec![0]);
    let missing4 = installer.missing_selections();
    assert!(
        missing4.is_empty(),
        "All groups should be satisfied after filling everything"
    );
}

// =====================================================================
// Declarative summary and diff
// =====================================================================

#[test]
fn e2e_declarative_summary_and_diff() {
    let _config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();

    // Config 1: High Resolution
    let decl_high = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(SIMPLE_CONFIG),
        selections: HashMap::from([
            (
                "Choose Textures".into(),
                HashMap::from([(
                    "Texture Quality".into(),
                    vec!["High Resolution".into()],
                )]),
            ),
            (
                "Optional Patches".into(),
                HashMap::from([(
                    "Performance Patches".into(),
                    vec!["LOD Optimization".into()],
                )]),
            ),
        ]),
    };

    // Config 2: Standard Resolution, no patches
    let decl_standard = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(SIMPLE_CONFIG),
        selections: HashMap::from([(
            "Choose Textures".into(),
            HashMap::from([(
                "Texture Quality".into(),
                vec!["Standard Resolution".into()],
            )]),
        )]),
    };

    // Test summary
    let summary_high = decl_high.summary();
    assert!(!summary_high.is_empty());
    // Summary should include human-readable entries
    let has_textures = summary_high
        .iter()
        .any(|s| s.step == "Choose Textures" && s.plugins.contains(&"High Resolution".into()));
    assert!(has_textures, "Summary should contain texture selection");

    // Test Display impl on summary entries
    let display_str = summary_high[0].to_string();
    assert!(!display_str.is_empty());
    assert!(display_str.contains(" > "));

    // Test diff
    let diffs = decl_high.diff(&decl_standard);
    assert!(!diffs.is_empty(), "There should be differences between configs");

    // Texture quality should differ
    let texture_diff = diffs
        .iter()
        .find(|d| d.group == "Texture Quality")
        .expect("Texture Quality should differ");
    assert_eq!(texture_diff.left, vec!["High Resolution".to_string()]);
    assert_eq!(texture_diff.right, vec!["Standard Resolution".to_string()]);

    // Performance Patches should differ (present in high, absent in standard)
    let patch_diff = diffs
        .iter()
        .find(|d| d.group == "Performance Patches")
        .expect("Performance Patches should differ");
    assert_eq!(patch_diff.left, vec!["LOD Optimization".to_string()]);
    assert!(patch_diff.right.is_empty());
}

// =====================================================================
// Full wizard with undo using HDTSMP fixture
// =====================================================================

#[test]
fn e2e_hdtsmp_wizard_with_undo() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Select SE + CUDA
    installer.select(1, 0, vec![0]); // SE
    installer.select(1, 1, vec![0]); // CUDA

    // Verify SE+CUDA conditional
    let plan = installer.resolve();
    assert!(plan.operations.iter().any(|op| op.source == "SE_CUDA\\SKSE"));

    // Checkpoint the SE+CUDA state
    installer.checkpoint();

    // Switch to AE (keep CUDA)
    installer.select(1, 0, vec![1]); // AE
    let plan_ae = installer.resolve();
    assert!(plan_ae.operations.iter().any(|op| op.source == "AE_CUDA\\SKSE"));
    assert!(!plan_ae.operations.iter().any(|op| op.source == "SE_CUDA\\SKSE"));

    // Rollback -> should restore SE+CUDA
    assert!(installer.rollback());

    // Verify SE+CUDA is restored
    let plan_restored = installer.resolve();
    assert!(
        plan_restored
            .operations
            .iter()
            .any(|op| op.source == "SE_CUDA\\SKSE"),
        "SE_CUDA should be restored after rollback"
    );
    assert!(
        !plan_restored
            .operations
            .iter()
            .any(|op| op.source == "AE_CUDA\\SKSE"),
        "AE_CUDA should not be present after rollback"
    );

    // Verify flags are restored (SSE=On, AE=Off)
    assert_eq!(installer.context().flags.get("SSE"), Some(&"On".to_string()));
    assert_eq!(installer.context().flags.get("AE"), Some(&"Off".to_string()));
}

// =====================================================================
// Metadata accessors in a real flow
// =====================================================================

#[test]
fn e2e_metadata_accessors() {
    let config = ModuleConfig::parse(SIMPLE_CONFIG).unwrap();
    let mut installer = Installer::new(config);

    // step_name
    assert_eq!(installer.step_name(0), Some("Choose Textures"));
    assert_eq!(installer.step_name(1), Some("Optional Patches"));
    assert_eq!(installer.step_name(99), None);

    // group_name
    assert_eq!(installer.group_name(0, 0), Some("Texture Quality"));
    assert_eq!(installer.group_name(1, 0), Some("Performance Patches"));
    assert_eq!(installer.group_name(0, 99), None);

    // plugin_description
    assert_eq!(
        installer.plugin_description(0, 0, 0),
        Some("4K textures for maximum quality")
    );
    assert_eq!(
        installer.plugin_description(0, 0, 1),
        Some("2K textures for balanced performance")
    );
    assert_eq!(installer.plugin_description(0, 0, 99), None);

    // plugin_image_path
    assert_eq!(installer.plugin_image_path(0, 0, 0), Some("images/hd.png"));
    assert_eq!(installer.plugin_image_path(0, 0, 1), None); // Standard has no image

    // module_image_path
    assert_eq!(installer.module_image_path(), Some("banner.png"));

    // plugin_type_at (static types before any selection context changes)
    use fomod_oxide::config::PluginType;
    assert_eq!(
        installer.plugin_type_at(0, 0, 0),
        Some(PluginType::Recommended)
    );
    assert_eq!(
        installer.plugin_type_at(0, 0, 1),
        Some(PluginType::Optional)
    );

    // group_type_at
    use fomod_oxide::config::GroupType;
    assert_eq!(
        installer.group_type_at(0, 0),
        Some(GroupType::SelectExactlyOne)
    );
    assert_eq!(
        installer.group_type_at(1, 0),
        Some(GroupType::SelectAny)
    );
    assert_eq!(installer.group_type_at(99, 0), None);

    // Select high-res and verify plugin_type_at still works in context
    installer.select(0, 0, vec![0]);
    assert_eq!(
        installer.plugin_type_at(0, 0, 0),
        Some(PluginType::Recommended)
    );
}
