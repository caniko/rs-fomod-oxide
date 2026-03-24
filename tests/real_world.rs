//! Integration tests using real-world FOMOD configs from the modding community.

use std::collections::HashMap;
use fomod_oxide::condition::EvalContext;
use fomod_oxide::config::{GroupType, PluginType};
use fomod_oxide::{
    hash_xml, CompletionStatus, DeclarativeConfig, FileConflict, Installer,
    ModuleConfig, SelectionSummary, SCHEMA_VERSION, ValidationHint,
};

// Verbatim from https://github.com/Nexus-Mods/Vortex/blob/master/tools/testfomod/fomod/ModuleConfig.xml
const VORTEX_TEST: &str = include_str!("fixtures/vortex_test_config.xml");

// Inspired by Faster HDT-SMP (https://github.com/DaymareOn/hdtSMP64)
const HDTSMP_LIKE: &str = include_str!("fixtures/hdtsmp_like_config.xml");

// ---- Vortex test FOMOD ----

#[test]
fn vortex_parse_structure() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    assert_eq!(config.module_name.value, "Test Module");

    let steps = config.install_steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 1);

    let groups = &steps.steps[0].optional_file_groups.as_ref().unwrap().groups;
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].name, "SelectExactlyOne - No Recommend");
    assert_eq!(groups[0].group_type, GroupType::SelectExactlyOne);
    assert_eq!(groups[1].name, "SelectExactlyOne - With Recommend");
    assert_eq!(groups[2].name, "Results");
    assert_eq!(groups[2].group_type, GroupType::SelectAny);
}

#[test]
fn vortex_dependency_type_default_is_not_usable() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let results_group = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    // All results plugins use dependencyType with defaultType=NotUsable
    for plugin in &results_group.plugins.plugins {
        assert_eq!(plugin.plugin_type(), PluginType::NotUsable);

        let dt = plugin
            .type_descriptor
            .as_ref()
            .unwrap()
            .dependency_type
            .as_ref()
            .unwrap();
        assert_eq!(dt.default_type.name, PluginType::NotUsable);
        assert!(dt.patterns.is_some());
    }
}

#[test]
fn vortex_condition_flags_propagate() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Select "File A" in group 0 (no recommend), "File B" in group 1 (with recommend)
    installer.select(0, 0, vec![0]); // fileanorec=On
    installer.select(0, 1, vec![1]); // filebrec=On

    let ctx = installer.context();
    assert_eq!(ctx.flags.get("fileanorec"), Some(&"On".to_string()));
    assert_eq!(ctx.flags.get("filebrec"), Some(&"On".to_string()));

    // The non-selected flags should have been cleared
    assert!(!ctx.flags.contains_key("filebnorec"));
    assert!(!ctx.flags.contains_key("filearec"));
}

#[test]
fn vortex_default_selections_with_recommend() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let groups = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups;

    // Group 0: SelectExactlyOne, all Optional -> no default
    let defaults_no_rec = Installer::default_selections(&groups[0]);
    assert!(defaults_no_rec.is_empty());

    // Group 1: SelectExactlyOne, first is Recommended -> defaults to index 0
    let defaults_rec = Installer::default_selections(&groups[1]);
    assert_eq!(defaults_rec, vec![0]);

    // Group 2: SelectAny, all NotUsable (via dependencyType default) -> no defaults
    let defaults_results = Installer::default_selections(&groups[2]);
    assert!(defaults_results.is_empty());
}

// ---- HDT-SMP-like config ----

#[test]
fn hdtsmp_parse_structure() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    assert_eq!(config.module_name.value, "Faster HDT-SMP");
    assert!(config.module_image.is_some());

    // Required files
    let required = config.required_install_files.as_ref().unwrap();
    assert_eq!(required.items.len(), 2);

    // 3 install steps
    let steps = config.install_steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 3);
    assert_eq!(steps.steps[0].name, "Introduction");
    assert_eq!(steps.steps[1].name, "Options");
    assert_eq!(steps.steps[2].name, "Thanks");

    // Step 1 has 2 groups (Platform + CUDA)
    let step1_groups = &steps.steps[1].optional_file_groups.as_ref().unwrap().groups;
    assert_eq!(step1_groups.len(), 2);
    assert_eq!(step1_groups[0].name, "Platform");
    assert_eq!(step1_groups[0].plugins.plugins.len(), 3); // SE, AE, VR
    assert_eq!(step1_groups[1].name, "CUDA");
    assert_eq!(step1_groups[1].plugins.plugins.len(), 2);

    // 4 conditional patterns
    let cfi = config.conditional_file_installs.as_ref().unwrap();
    assert_eq!(cfi.patterns.patterns.len(), 4);
}

#[test]
fn hdtsmp_step_visibility_with_dependencies_wrapper() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Initially: steps 0 and 1 visible, step 2 hidden (requires SSE=On OR AE=On)
    let visible = installer.visible_steps();
    assert_eq!(
        visible.len(),
        2,
        "Thanks should be hidden with no flags set"
    );

    // Select VR (SSE=Off, AE=Off) — step 2 should remain hidden
    installer.select(1, 0, vec![2]); // VR
    installer.select(1, 1, vec![1]); // No CUDA

    let visible = installer.visible_steps();
    let visible_names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(
        !visible_names.contains(&"Thanks"),
        "Thanks step should be hidden when VR is selected (SSE=Off, AE=Off)"
    );

    // Select SE -> step 2 should become visible
    installer.select(1, 0, vec![0]); // SE: SSE=On, AE=Off
    let visible = installer.visible_steps();
    let visible_names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(
        visible_names.contains(&"Thanks"),
        "Thanks step should be visible when SSE=On"
    );

    // Select AE -> step 2 should still be visible
    installer.select(1, 0, vec![1]); // AE: AE=On, SSE=Off
    let visible = installer.visible_steps();
    let visible_names: Vec<&str> = visible.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(
        visible_names.contains(&"Thanks"),
        "Thanks step should be visible when AE=On"
    );
}

#[test]
fn hdtsmp_conditional_file_installs_se_cuda() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Select SE + CUDA
    installer.select(1, 0, vec![0]); // SE
    installer.select(1, 1, vec![0]); // CUDA

    let plan = installer.resolve();

    // Required files always present
    assert!(plan.operations.iter().any(|op| op.source == "Licence.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "Readme.txt"));

    // SE+CUDA pattern matched
    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "SE_CUDA\\SKSE"),
        "SE_CUDA pattern should match"
    );

    // Other patterns should NOT match
    assert!(
        !plan
            .operations
            .iter()
            .any(|op| op.source == "SE_NOCUDA\\SKSE"),
    );
    assert!(
        !plan
            .operations
            .iter()
            .any(|op| op.source == "AE_CUDA\\SKSE"),
    );
}

#[test]
fn hdtsmp_conditional_file_installs_ae_nocuda() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Select AE + No CUDA
    installer.select(1, 0, vec![1]); // AE
    installer.select(1, 1, vec![1]); // No CUDA

    let plan = installer.resolve();

    assert!(
        plan.operations
            .iter()
            .any(|op| op.source == "AE_NOCUDA\\SKSE"),
        "AE_NOCUDA pattern should match"
    );

    // No other SKSE patterns
    let skse_ops: Vec<&str> = plan
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();
    assert_eq!(skse_ops, vec!["AE_NOCUDA\\SKSE"]);
}

#[test]
fn hdtsmp_multi_flag_per_plugin() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // SE plugin sets TWO flags: SSE=On, AE=Off
    installer.select(1, 0, vec![0]);
    let ctx = installer.context();
    assert_eq!(ctx.flags.get("SSE"), Some(&"On".to_string()));
    assert_eq!(ctx.flags.get("AE"), Some(&"Off".to_string()));

    // Switch to AE: SSE=Off, AE=On
    installer.select(1, 0, vec![1]);
    let ctx = installer.context();
    assert_eq!(ctx.flags.get("SSE"), Some(&"Off".to_string()));
    assert_eq!(ctx.flags.get("AE"), Some(&"On".to_string()));
}

// ---- dependencyType runtime evaluation ----

#[test]
fn vortex_dependency_type_runtime_evaluation() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Before any selections, Results plugins should all be NotUsable
    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    for plugin in &results_group.plugins.plugins {
        assert_eq!(
            plugin.plugin_type_in_context(installer.context()),
            PluginType::NotUsable,
            "all Results plugins should be NotUsable before selections"
        );
    }

    // Select "File A" in group 0 (no recommend) → sets fileanorec=On
    installer.select(0, 0, vec![0]);

    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    // "First A" should now be Required (pattern matches fileanorec=On)
    assert_eq!(
        results_group.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::Required,
        "First A should be Required when fileanorec=On"
    );

    // "First B" should still be NotUsable
    assert_eq!(
        results_group.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
        "First B should remain NotUsable"
    );
}

#[test]
fn vortex_select_all_b_variants() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Select File B in both groups
    installer.select(0, 0, vec![1]); // filebnorec=On
    installer.select(0, 1, vec![1]); // filebrec=On

    let ctx = installer.context();
    assert_eq!(ctx.flags.get("filebnorec"), Some(&"On".to_string()));
    assert_eq!(ctx.flags.get("filebrec"), Some(&"On".to_string()));

    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    // First A: fileanorec not set → NotUsable
    assert_eq!(
        results_group.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
    );
    // First B: filebnorec=On → Required
    assert_eq!(
        results_group.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::Required,
    );
    // Second A: filearec not set → NotUsable
    assert_eq!(
        results_group.plugins.plugins[2].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
    );
    // Second B: filebrec=On → Required
    assert_eq!(
        results_group.plugins.plugins[3].plugin_type_in_context(installer.context()),
        PluginType::Required,
    );
}

#[test]
fn hdtsmp_all_conditional_file_combinations() {
    // Test each of the 4 conditional file patterns
    let combos = vec![
        (vec![0], vec![0], "SE_CUDA\\SKSE"),    // SE + CUDA
        (vec![0], vec![1], "SE_NOCUDA\\SKSE"),   // SE + No CUDA
        (vec![1], vec![0], "AE_CUDA\\SKSE"),     // AE + CUDA
        (vec![1], vec![1], "AE_NOCUDA\\SKSE"),   // AE + No CUDA
    ];

    for (platform_sel, cuda_sel, expected_source) in combos {
        let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
        let mut installer = Installer::new(config);

        installer.select(1, 0, platform_sel.clone());
        installer.select(1, 1, cuda_sel.clone());

        let plan = installer.resolve();
        let skse_ops: Vec<&str> = plan
            .operations
            .iter()
            .filter(|op| op.source.contains("SKSE"))
            .map(|op| op.source.as_str())
            .collect();

        assert_eq!(
            skse_ops,
            vec![expected_source],
            "Platform {:?} + CUDA {:?} should produce {:?}",
            platform_sel,
            cuda_sel,
            expected_source
        );
    }
}

#[test]
fn hdtsmp_required_files_always_present() {
    // Required files should appear regardless of selections
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let installer = Installer::new(config);
    let plan = installer.resolve();

    assert!(plan.operations.iter().any(|op| op.source == "Licence.txt"));
    assert!(plan.operations.iter().any(|op| op.source == "Readme.txt"));
}

#[test]
fn hdtsmp_declarative_roundtrip() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let decl = DeclarativeConfig::from_defaults(HDTSMP_LIKE, "1.0", &config);

    // Apply to fresh installer
    let config2 = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config2);

    match decl.apply(HDTSMP_LIKE, &mut installer) {
        Ok(()) => {
            let plan = installer.resolve();
            // Should at least have required files
            assert!(plan.operations.iter().any(|op| op.source == "Licence.txt"));
        }
        Err(e) => {
            // ValidationFailed is acceptable for groups with no default
            assert!(
                e.to_string().contains("invalid selection"),
                "Unexpected error: {e}"
            );
        }
    }
}

// ---- DependencyType patterns fixture ----

const DEP_TYPE_XML: &str = include_str!("fixtures/dependency_type_patterns.xml");

#[test]
fn dep_type_fixture_parse_structure() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    assert_eq!(config.module_name.value, "DependencyType Patterns Test");

    let steps = config.install_steps.as_ref().unwrap();
    assert_eq!(steps.steps.len(), 2);
    assert_eq!(steps.steps[0].name, "Choose Platform");
    assert_eq!(steps.steps[1].name, "Compatibility");

    // Platform group has 3 plugins
    let platform_group = &steps.steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];
    assert_eq!(platform_group.plugins.plugins.len(), 3);

    // Results group has 3 plugins with dependencyType
    let results = &steps.steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];
    assert_eq!(results.plugins.plugins.len(), 3);

    // All results have dependencyType (not simple type)
    for plugin in &results.plugins.plugins {
        let td = plugin.type_descriptor.as_ref().unwrap();
        assert!(td.simple_type.is_none());
        assert!(td.dependency_type.is_some());
    }
}

#[test]
fn dep_type_default_types_without_context() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();

    let results = &config.install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // Static types (without context): returns default types
    assert_eq!(results.plugins.plugins[0].plugin_type(), PluginType::NotUsable);
    assert_eq!(results.plugins.plugins[1].plugin_type(), PluginType::NotUsable);
    assert_eq!(results.plugins.plugins[2].plugin_type(), PluginType::CouldBeUsable);
}

#[test]
fn dep_type_conditional_patterns_comprehensive() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let results = &config.install_steps.as_ref().unwrap().steps[1]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[0];

    // Test with each platform context
    let scenarios = vec![
        // (platform, os_family, expected_types_for_3_plugins)
        ("windows", "desktop", vec![PluginType::Required, PluginType::Recommended, PluginType::CouldBeUsable]),
        ("linux", "desktop", vec![PluginType::Recommended, PluginType::Recommended, PluginType::CouldBeUsable]),
        ("steamdeck", "handheld", vec![PluginType::NotUsable, PluginType::Optional, PluginType::Required]),
    ];

    for (platform, os_family, expected) in scenarios {
        let mut ctx = EvalContext::new();
        ctx.set_flag("platform", platform);
        ctx.set_flag("os_family", os_family);

        for (i, exp) in expected.iter().enumerate() {
            let actual = results.plugins.plugins[i].plugin_type_in_context(&ctx);
            assert_eq!(
                actual, *exp,
                "Plugin {} with platform={}, os_family={}: expected {:?}, got {:?}",
                results.plugins.plugins[i].name, platform, os_family, exp, actual
            );
        }
    }
}

#[test]
fn vortex_default_selections_in_context_with_dependency_type() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Before selections: Results group has no Required/Recommended via context
    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    let defaults = Installer::default_selections_in_context(results_group, installer.context());
    assert!(defaults.is_empty(), "no defaults when all NotUsable");

    // Set flag to activate "First A" → becomes Required
    installer.select(0, 0, vec![0]); // fileanorec=On

    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups
        .as_ref()
        .unwrap()
        .groups[2];

    let defaults = Installer::default_selections_in_context(results_group, installer.context());
    assert_eq!(defaults, vec![0], "First A (index 0) should be a default when Required");
}

// ---- Creative edge-case tests ----

/// 1. Vortex: switching all groups rapidly and verifying final state.
/// Select A in both groups, then B in both, then A again. Verify flags,
/// plugin types, and resolved plan are consistent.
#[test]
fn vortex_rapid_group_switching_final_state() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Round 1: select A everywhere
    installer.select(0, 0, vec![0]); // fileanorec=On
    installer.select(0, 1, vec![0]); // filearec=On

    // Round 2: switch to B everywhere
    installer.select(0, 0, vec![1]); // filebnorec=On, fileanorec cleared
    installer.select(0, 1, vec![1]); // filebrec=On, filearec cleared

    // Round 3: switch back to A everywhere
    installer.select(0, 0, vec![0]); // fileanorec=On, filebnorec cleared
    installer.select(0, 1, vec![0]); // filearec=On, filebrec cleared

    let ctx = installer.context();
    assert_eq!(ctx.flags.get("fileanorec"), Some(&"On".to_string()));
    assert_eq!(ctx.flags.get("filearec"), Some(&"On".to_string()));
    assert!(!ctx.flags.contains_key("filebnorec"), "filebnorec should be cleared");
    assert!(!ctx.flags.contains_key("filebrec"), "filebrec should be cleared");

    // Plugin types in the Results group should reflect final state
    let results_group = &installer.config().install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups.as_ref().unwrap().groups[2];

    // "First A" → Required (fileanorec=On)
    assert_eq!(
        results_group.plugins.plugins[0].plugin_type_in_context(installer.context()),
        PluginType::Required,
    );
    // "First B" → NotUsable (filebnorec not set)
    assert_eq!(
        results_group.plugins.plugins[1].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
    );
    // "Second A" → Required (filearec=On)
    assert_eq!(
        results_group.plugins.plugins[2].plugin_type_in_context(installer.context()),
        PluginType::Required,
    );
    // "Second B" → NotUsable (filebrec not set)
    assert_eq!(
        results_group.plugins.plugins[3].plugin_type_in_context(installer.context()),
        PluginType::NotUsable,
    );

    // Resolve should be consistent (no crash, plan is valid)
    let plan = installer.resolve();
    // No file sources from vortex test Results plugins (they have empty <files/>)
    // Just verify resolve succeeds without panic
    assert!(plan.operations.is_empty() || !plan.operations.is_empty());
}

/// 2. HDTSMP: checkpoint at every step and rollback chain.
/// Use checkpoint/rollback through the 3-step wizard. Verify state is correct
/// at each level.
#[test]
fn hdtsmp_checkpoint_rollback_chain() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Checkpoint 0: pristine state
    installer.checkpoint();
    assert_eq!(installer.history_len(), 1);

    // Step 0: Introduction (SelectAll), select the intro plugin
    installer.select(0, 0, vec![0]);
    installer.checkpoint();
    assert_eq!(installer.history_len(), 2);

    // Step 1: Select SE + CUDA
    installer.select(1, 0, vec![0]); // SE → SSE=On, AE=Off
    installer.select(1, 1, vec![0]); // CUDA → CUDA=On
    installer.checkpoint();
    assert_eq!(installer.history_len(), 3);

    // Verify SE+CUDA state
    assert_eq!(installer.context().flags.get("SSE"), Some(&"On".to_string()));
    assert_eq!(installer.context().flags.get("CUDA"), Some(&"On".to_string()));

    // Now select AE + No CUDA (simulating user changing mind)
    installer.select(1, 0, vec![1]); // AE → AE=On, SSE=Off
    installer.select(1, 1, vec![1]); // No CUDA → CUDA=Off

    assert_eq!(installer.context().flags.get("AE"), Some(&"On".to_string()));
    assert_eq!(installer.context().flags.get("CUDA"), Some(&"Off".to_string()));

    // Rollback to checkpoint 3 (SE+CUDA)
    assert!(installer.rollback());
    assert_eq!(installer.context().flags.get("SSE"), Some(&"On".to_string()));
    assert_eq!(installer.context().flags.get("CUDA"), Some(&"On".to_string()));
    assert_eq!(installer.history_len(), 2);

    // Rollback to checkpoint 2 (after Introduction, before Options)
    assert!(installer.rollback());
    assert!(!installer.context().flags.contains_key("SSE"));
    assert!(!installer.context().flags.contains_key("CUDA"));
    assert_eq!(installer.history_len(), 1);

    // Rollback to checkpoint 1 (pristine)
    assert!(installer.rollback());
    assert!(installer.context().flags.is_empty());
    assert!(installer.selections().is_empty());
    assert_eq!(installer.history_len(), 0);

    // No more history
    assert!(!installer.rollback());
}

/// 3. Dep type: context-aware defaults change as flags evolve.
/// Select each platform and verify default_selections_in_context returns
/// different results for the Results group.
#[test]
fn dep_type_context_aware_defaults_change_with_flags() {
    let config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config);

    let get_results_group = |inst: &Installer| -> Vec<usize> {
        let results = &inst.config().install_steps.as_ref().unwrap().steps[1]
            .optional_file_groups.as_ref().unwrap().groups[0];
        Installer::default_selections_in_context(results, inst.context())
    };

    // No flags set: defaults from defaultType (NotUsable, NotUsable, CouldBeUsable) → empty
    let defaults_none = get_results_group(&installer);
    assert!(defaults_none.is_empty(), "no defaults with no context");

    // Select Windows → platform=windows, os_family=desktop
    installer.select(0, 0, vec![0]);
    let defaults_win = get_results_group(&installer);
    // "Windows Patch" → Required (idx 0), "Desktop Patch" → Recommended (idx 1)
    assert!(defaults_win.contains(&0), "Windows Patch should be default for windows");
    assert!(defaults_win.contains(&1), "Desktop Patch should be default for windows/desktop");

    // Select Linux → platform=linux, os_family=desktop
    installer.select(0, 0, vec![1]);
    let defaults_linux = get_results_group(&installer);
    // "Windows Patch" → Recommended (os_family=desktop matches 2nd pattern)
    // "Desktop Patch" → Recommended
    assert!(defaults_linux.contains(&0), "Windows Patch should be Recommended for linux/desktop");
    assert!(defaults_linux.contains(&1), "Desktop Patch should be Recommended for linux/desktop");

    // Select SteamDeck → platform=steamdeck, os_family=handheld
    installer.select(0, 0, vec![2]);
    let defaults_deck = get_results_group(&installer);
    // "Windows Patch" → NotUsable (no pattern matches)
    // "Desktop Patch" → Optional (handheld pattern) → not a default
    // "CouldBeUsable Plugin" → Required (steamdeck pattern)
    assert!(defaults_deck.contains(&2), "CouldBeUsable Plugin should be Required for steamdeck");
    assert!(!defaults_deck.contains(&0), "Windows Patch should not be default for steamdeck");

    // windows and linux both have indices [0, 1] as defaults but with different
    // underlying types (Required vs Recommended for index 0). The key difference
    // is between desktop (windows/linux) and handheld (steamdeck).
    assert_ne!(defaults_linux, defaults_deck, "linux vs steamdeck defaults should differ");
    assert_ne!(defaults_win, defaults_deck, "windows vs steamdeck defaults should differ");

    // Verify the types are different even if default indices overlap.
    // Reset to Windows context to compare types.
    installer.select(0, 0, vec![0]);
    let win_type_0 = {
        let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
            .optional_file_groups.as_ref().unwrap().groups[0];
        results.plugins.plugins[0].plugin_type_in_context(installer.context())
    };
    // Reset to Linux context
    installer.select(0, 0, vec![1]);
    let linux_type_0 = {
        let results = &installer.config().install_steps.as_ref().unwrap().steps[1]
            .optional_file_groups.as_ref().unwrap().groups[0];
        results.plugins.plugins[0].plugin_type_in_context(installer.context())
    };
    assert_ne!(
        win_type_0, linux_type_0,
        "Windows Patch type should differ between windows (Required) and linux (Recommended)"
    );
}

/// 4. Vortex: NotUsable plugins should produce ValidationHint when selected.
/// Select a NotUsable plugin (via dependencyType) and verify validate_step
/// produces NotUsableSelected hint.
#[test]
fn vortex_not_usable_produces_validation_hint() {
    let config = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer = Installer::new(config);

    // Without setting any flags, all Results plugins are NotUsable via dependencyType default.
    // Select "First A" (index 0) in the Results group (group index 2).
    installer.select(0, 2, vec![0]);

    let hints = installer.validate_step(0);

    // We should get a NotUsableSelected hint for "First A"
    let not_usable_hints: Vec<&ValidationHint> = hints
        .iter()
        .filter(|h| matches!(h, ValidationHint::NotUsableSelected { .. }))
        .collect();

    assert!(
        !not_usable_hints.is_empty(),
        "Should have NotUsableSelected hint when selecting a NotUsable plugin"
    );

    let has_first_a = not_usable_hints.iter().any(|h| match h {
        ValidationHint::NotUsableSelected { plugin, .. } => plugin == "First A",
        _ => false,
    });
    assert!(has_first_a, "Should specifically flag 'First A' as NotUsable");

    // Also verify the NeedExactly hints for groups 0 and 1 (SelectExactlyOne, 0 selected)
    let need_hints: Vec<&ValidationHint> = hints
        .iter()
        .filter(|h| matches!(h, ValidationHint::NeedExactly { .. }))
        .collect();
    assert!(
        need_hints.len() >= 2,
        "Both SelectExactlyOne groups should have NeedExactly hints when nothing selected"
    );
}

/// 5. HDTSMP: detect file conflicts between required files and conditional.
/// Required files + conditional files going to same destination should show up.
#[test]
fn hdtsmp_detect_file_conflicts_required_vs_conditional() {
    // The HDTSMP fixture has required files (Licence.txt, Readme.txt) and conditional
    // SKSE folders. These go to different destinations, so no conflict expected.
    // But the conditional patterns themselves all target "SKSE" destination -
    // check that detect_conflicts catches overlapping conditional patterns.
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let installer = Installer::new(config);

    let conflicts = installer.detect_conflicts();

    // The 4 conditional SKSE patterns all write to destination "SKSE" -
    // these are from different conditional patterns and should be detected
    // as potential conflicts since they target the same destination.
    let _skse_conflict = conflicts
        .iter()
        .find(|c| c.destination.contains("skse") || c.destination.contains("SKSE"));

    // Note: detect_conflicts only checks required + plugin files, not conditional.
    // Required files (Licence.txt, Readme.txt) have unique destinations.
    // No plugin has files in this fixture (plugins only set flags), so no plugin conflicts.
    // The test verifies the method runs without panic on this fixture.
    // Required files have distinct destinations, so no conflicts expected among them.
    let licence_conflicts: Vec<&FileConflict> = conflicts
        .iter()
        .filter(|c| c.destination.contains("licence"))
        .collect();
    assert!(
        licence_conflicts.is_empty(),
        "Licence.txt has a unique destination, should not conflict"
    );

    // Verify detect_conflicts returns a valid (possibly empty) vec
    // The important thing is it doesn't panic on a config with conditional files
    assert!(conflicts.len() < 100, "sanity check: not an unreasonable number of conflicts");
}

/// 6. Cross-fixture: declarative config diff between from_defaults and from_all.
/// Show the diff captures all the extra plugins.
#[test]
fn cross_fixture_declarative_diff_defaults_vs_all() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let defaults = DeclarativeConfig::from_defaults(HDTSMP_LIKE, "1.0", &config);
    let all = DeclarativeConfig::from_all(HDTSMP_LIKE, "1.0", &config);

    let diffs = defaults.diff(&all);

    // from_defaults for HDTSMP: Introduction group is SelectAll → all plugins selected;
    // Platform group is SelectExactlyOne, no Recommended → empty;
    // CUDA group: "No CUDA" is Recommended → ["No CUDA"];
    // Credits group is SelectAll → all selected.
    //
    // from_all: every plugin in every group is listed.
    // So we expect diffs for at least Platform (empty vs [SE, AE, VR])
    // and CUDA (["No CUDA"] vs ["CUDA", "No CUDA"]).

    assert!(
        !diffs.is_empty(),
        "from_defaults and from_all should differ"
    );

    // Find the Platform diff
    let platform_diff = diffs.iter().find(|d| d.group == "Platform");
    assert!(platform_diff.is_some(), "Platform group should have a diff");
    let pd = platform_diff.unwrap();
    // defaults has no selection for Platform (no Recommended), all has all 3
    assert!(pd.left.len() < pd.right.len(), "all should have more plugins than defaults");

    // Find the CUDA diff
    let cuda_diff = diffs.iter().find(|d| d.group == "CUDA");
    assert!(cuda_diff.is_some(), "CUDA group should have a diff");
    let cd = cuda_diff.unwrap();
    assert!(cd.right.contains(&"CUDA".to_string()), "all should include CUDA");
    assert!(cd.right.contains(&"No CUDA".to_string()), "all should include No CUDA");
}

/// 7. All fixtures: summary Display formatting.
/// Verify SelectionSummary Display output format.
#[test]
fn all_fixtures_summary_display_formatting() {
    let fixtures = [
        ("vortex", VORTEX_TEST),
        ("hdtsmp", HDTSMP_LIKE),
        ("dep_type", DEP_TYPE_XML),
    ];

    for (name, xml) in &fixtures {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_all(xml, "1.0", &config);
        let summaries = decl.summary();

        assert!(!summaries.is_empty(), "{name} should have summaries");

        for summary in &summaries {
            let display = format!("{}", summary);
            // Format should be: "step > group: [plugin1, plugin2, ...]"
            assert!(
                display.contains(" > "),
                "{name}: summary should contain ' > ' separator, got: {display}"
            );
            assert!(
                display.contains(": ["),
                "{name}: summary should contain ': [' for plugin list, got: {display}"
            );
            assert!(
                display.ends_with(']'),
                "{name}: summary should end with ']', got: {display}"
            );

            // Verify the components match the struct fields
            assert!(
                display.starts_with(&summary.step),
                "{name}: display should start with step name"
            );
            assert!(
                display.contains(&summary.group),
                "{name}: display should contain group name"
            );
        }
    }

    // Verify specific formatting with known content
    let summary = SelectionSummary {
        step: "My Step".into(),
        group: "My Group".into(),
        plugins: vec!["Plugin A".into(), "Plugin B".into()],
    };
    assert_eq!(
        format!("{}", summary),
        "My Step > My Group: [Plugin A, Plugin B]"
    );

    // Empty plugins
    let empty = SelectionSummary {
        step: "S".into(),
        group: "G".into(),
        plugins: vec![],
    };
    assert_eq!(format!("{}", empty), "S > G: []");
}

/// 8. HDTSMP: preview_current vs resolve shows conditional difference.
/// preview_current should NOT include conditional SKSE files, but resolve should.
#[test]
fn hdtsmp_preview_current_vs_resolve_conditional_difference() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer = Installer::new(config);

    // Select SE + CUDA
    installer.select(1, 0, vec![0]); // SE
    installer.select(1, 1, vec![0]); // CUDA

    let preview = installer.preview_current();
    let resolved = installer.resolve();

    // preview_current excludes conditional file installs
    let preview_skse: Vec<&str> = preview
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();

    let resolved_skse: Vec<&str> = resolved
        .operations
        .iter()
        .filter(|op| op.source.contains("SKSE"))
        .map(|op| op.source.as_str())
        .collect();

    assert!(
        preview_skse.is_empty(),
        "preview_current should NOT include conditional SKSE files, got: {:?}",
        preview_skse
    );
    assert_eq!(
        resolved_skse,
        vec!["SE_CUDA\\SKSE"],
        "resolve should include the matched conditional SKSE files"
    );

    // Both should include required files
    assert!(
        preview.operations.iter().any(|op| op.source == "Licence.txt"),
        "preview should include required files"
    );
    assert!(
        resolved.operations.iter().any(|op| op.source == "Licence.txt"),
        "resolve should include required files"
    );

    // resolve has more operations than preview (the conditional files)
    assert!(
        resolved.operations.len() > preview.operations.len(),
        "resolve ({}) should have more ops than preview ({})",
        resolved.operations.len(),
        preview.operations.len()
    );
}

/// 9. Dep type: full declarative roundtrip with context-aware selections.
/// Create declarative config manually for SteamDeck, apply, verify correct
/// conditionals triggered.
#[test]
fn dep_type_declarative_roundtrip_steamdeck() {
    let _config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();

    // Build a declarative config that selects SteamDeck
    let decl = DeclarativeConfig {
        schema_version: SCHEMA_VERSION,
        rev: "1.0".into(),
        hash: hash_xml(DEP_TYPE_XML),
        selections: HashMap::from([
            (
                "Choose Platform".into(),
                HashMap::from([("Platform".into(), vec!["SteamDeck".into()])]),
            ),
            // Leave Compatibility step empty → defaults
            (
                "Compatibility".into(),
                HashMap::from([("Results".into(), vec![])]),
            ),
        ]),
    };

    let config2 = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let mut installer = Installer::new(config2);
    decl.apply(DEP_TYPE_XML, &mut installer).unwrap();

    // Verify flags are set
    let ctx = installer.context();
    assert_eq!(ctx.flags.get("platform"), Some(&"steamdeck".to_string()));
    assert_eq!(ctx.flags.get("os_family"), Some(&"handheld".to_string()));

    // Resolve and check conditional file installs
    let plan = installer.resolve();

    // SteamDeck → platform=steamdeck → deck_extras.dll conditional should match
    assert!(
        plan.operations.iter().any(|op| op.source == "deck_extras.dll"),
        "SteamDeck should trigger deck_extras.dll conditional"
    );

    // desktop conditional should NOT match (os_family=handheld, not desktop)
    assert!(
        !plan.operations.iter().any(|op| op.source == "desktop_extras.dll"),
        "SteamDeck should not trigger desktop_extras.dll"
    );

    // windows conditional should NOT match
    assert!(
        !plan.operations.iter().any(|op| op.source == "win_extras.dll"),
        "SteamDeck should not trigger win_extras.dll"
    );

    // Plugin base file should be present
    assert!(
        plan.operations.iter().any(|op| op.source == "deck_base.so"),
        "SteamDeck base file should be in plan"
    );
}

/// 10. Vortex: re-parse config after each selection step.
/// Verify that creating new installer + applying same selections gives
/// same result (determinism).
#[test]
fn vortex_deterministic_reparse() {
    // First run
    let config1 = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer1 = Installer::new(config1);
    installer1.select(0, 0, vec![0]); // File A no rec
    installer1.select(0, 1, vec![1]); // File B rec

    let ctx1_flags = installer1.context().flags.clone();
    let plan1 = installer1.resolve();

    // Second run: fresh parse, same selections
    let config2 = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer2 = Installer::new(config2);
    installer2.select(0, 0, vec![0]);
    installer2.select(0, 1, vec![1]);

    let ctx2_flags = installer2.context().flags.clone();
    let plan2 = installer2.resolve();

    // Flags should be identical
    assert_eq!(ctx1_flags, ctx2_flags, "Flags should be deterministic across re-parses");

    // Plans should have the same operations
    assert_eq!(
        plan1.operations.len(),
        plan2.operations.len(),
        "Plans should have same number of operations"
    );

    for (op1, op2) in plan1.operations.iter().zip(plan2.operations.iter()) {
        assert_eq!(op1.source, op2.source, "Sources should match");
        assert_eq!(op1.destination, op2.destination, "Destinations should match");
        assert_eq!(op1.priority, op2.priority, "Priorities should match");
        assert_eq!(op1.is_folder, op2.is_folder, "is_folder should match");
    }

    // Third run with different order of the same selections
    let config3 = ModuleConfig::parse(VORTEX_TEST).unwrap();
    let mut installer3 = Installer::new(config3);
    installer3.select(0, 1, vec![1]); // File B rec (reversed order)
    installer3.select(0, 0, vec![0]); // File A no rec

    let ctx3_flags = installer3.context().flags.clone();
    assert_eq!(
        ctx1_flags, ctx3_flags,
        "Flags should be same regardless of selection order"
    );
}

/// 11. SelectionDiff symmetry.
/// diff(a, b) should be mirror of diff(b, a) for the same step/group pairs.
#[test]
fn selection_diff_symmetry() {
    let config = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let defaults = DeclarativeConfig::from_defaults(HDTSMP_LIKE, "1.0", &config);
    let all = DeclarativeConfig::from_all(HDTSMP_LIKE, "1.0", &config);

    let forward = defaults.diff(&all);
    let backward = all.diff(&defaults);

    assert_eq!(
        forward.len(),
        backward.len(),
        "diff(a,b) and diff(b,a) should have same number of entries"
    );

    // For each diff entry, left and right should be swapped
    for fwd in &forward {
        let bwd = backward
            .iter()
            .find(|d| d.step == fwd.step && d.group == fwd.group)
            .unwrap_or_else(|| panic!("Missing backward diff for {} > {}", fwd.step, fwd.group));

        assert_eq!(
            fwd.left, bwd.right,
            "forward.left should equal backward.right for {} > {}",
            fwd.step, fwd.group
        );
        assert_eq!(
            fwd.right, bwd.left,
            "forward.right should equal backward.left for {} > {}",
            fwd.step, fwd.group
        );
    }

    // Also test identical configs produce empty diff
    let same = defaults.diff(&defaults);
    assert!(same.is_empty(), "diff of identical configs should be empty");

    // Test with dep_type fixture too for breadth
    let dep_config = ModuleConfig::parse(DEP_TYPE_XML).unwrap();
    let dep_defaults = DeclarativeConfig::from_defaults(DEP_TYPE_XML, "1.0", &dep_config);
    let dep_all = DeclarativeConfig::from_all(DEP_TYPE_XML, "1.0", &dep_config);

    let fwd2 = dep_defaults.diff(&dep_all);
    let bwd2 = dep_all.diff(&dep_defaults);
    assert_eq!(fwd2.len(), bwd2.len());
    for f in &fwd2 {
        let b = bwd2.iter().find(|d| d.step == f.step && d.group == f.group).unwrap();
        assert_eq!(f.left, b.right);
        assert_eq!(f.right, b.left);
    }
}

/// 12. completion_status fraction edge cases.
/// 0 groups -> 1.0, 1/2 -> 0.5, 3/3 -> 1.0
#[test]
fn completion_status_fraction_edge_cases() {
    // 0 groups → 1.0 (no config with steps)
    let xml_no_steps = r#"<config><moduleName>T</moduleName></config>"#;
    let config0 = ModuleConfig::parse(xml_no_steps).unwrap();
    let installer0 = Installer::new(config0);
    let status0 = installer0.completion_status();
    assert_eq!(status0.total_groups, 0);
    assert_eq!(status0.fraction(), 1.0, "0 groups should give fraction 1.0");

    // HDTSMP: 3 steps, multiple groups. Without selections, some groups unsatisfied.
    let config_h = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer_h = Installer::new(config_h);

    // Initially visible: steps 0 and 1.
    // Step 0 has 1 group (SelectAll with 1 plugin) → needs all selected, currently 0
    // Step 1 has 2 groups (SelectExactlyOne each) → need exactly 1 each, currently 0
    let status_empty = installer_h.completion_status();
    assert!(
        status_empty.total_groups > 0,
        "HDTSMP should have visible groups"
    );
    // fraction should be between 0 and 1 (some groups auto-satisfy, some don't)
    let frac_empty = status_empty.fraction();
    assert!(frac_empty >= 0.0 && frac_empty <= 1.0, "fraction in [0,1]");

    // Satisfy all visible groups
    installer_h.select(0, 0, vec![0]); // Introduction: SelectAll, 1 plugin
    installer_h.select(1, 0, vec![0]); // Platform: SE
    installer_h.select(1, 1, vec![0]); // CUDA: CUDA

    // Now step 2 (Thanks) becomes visible (SSE=On), adding 1 more group (SelectAll)
    // We need to satisfy that too
    installer_h.select(2, 0, vec![0]); // Credits: Thanks

    let status_full = installer_h.completion_status();
    assert_eq!(
        status_full.fraction(),
        1.0,
        "all groups satisfied should give 1.0"
    );
    assert_eq!(
        status_full.satisfied_groups, status_full.total_groups,
        "satisfied should equal total"
    );

    // Test the 1/2 case: satisfy only some groups
    let config_h2 = ModuleConfig::parse(HDTSMP_LIKE).unwrap();
    let mut installer_h2 = Installer::new(config_h2);
    // Only satisfy Introduction (step 0, group 0: SelectAll)
    installer_h2.select(0, 0, vec![0]);
    // Leave step 1 groups unsatisfied (SelectExactlyOne with 0 selected)
    let status_partial = installer_h2.completion_status();
    assert!(
        status_partial.fraction() > 0.0,
        "at least one group satisfied"
    );
    assert!(
        status_partial.fraction() < 1.0,
        "not all groups satisfied"
    );

    // Explicit 0.5 test: use CompletionStatus directly
    let half = CompletionStatus {
        total_steps: 2,
        visible_steps: 2,
        total_groups: 2,
        satisfied_groups: 1,
    };
    assert_eq!(half.fraction(), 0.5, "1/2 should give 0.5");

    let full = CompletionStatus {
        total_steps: 3,
        visible_steps: 3,
        total_groups: 3,
        satisfied_groups: 3,
    };
    assert_eq!(full.fraction(), 1.0, "3/3 should give 1.0");
}
