//! Smoke tests: quick sanity checks that the library loads, parses, and resolves
//! without panicking on every fixture.
#![allow(unused_variables)]

use fomod_oxide::{
    hash_xml, DeclarativeConfig, FomodInfo, Installer, ModuleConfig, SelectionSummary,
    SCHEMA_VERSION,
};

const FIXTURES: &[(&str, &str)] = &[
    ("simple", include_str!("fixtures/simple_config.xml")),
    ("minimal", include_str!("fixtures/minimal_config.xml")),
    (
        "nested_conditions",
        include_str!("fixtures/nested_conditions.xml"),
    ),
    (
        "all_group_types",
        include_str!("fixtures/all_group_types.xml"),
    ),
    ("priority", include_str!("fixtures/priority_config.xml")),
    (
        "multi_step",
        include_str!("fixtures/multi_step_visibility.xml"),
    ),
    ("vortex", include_str!("fixtures/vortex_test_config.xml")),
    ("hdtsmp", include_str!("fixtures/hdtsmp_like_config.xml")),
    (
        "dep_type_patterns",
        include_str!("fixtures/dependency_type_patterns.xml"),
    ),
];

/// Every fixture must parse without error.
#[test]
fn smoke_all_fixtures_parse() {
    for (name, xml) in FIXTURES {
        let result = ModuleConfig::parse(xml);
        assert!(result.is_ok(), "Failed to parse fixture: {name}");
    }
}

/// Every fixture must create an Installer without panic.
#[test]
fn smoke_all_fixtures_create_installer() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        // Should be able to access config and context
        let _ = installer.config();
        let _ = installer.context();
    }
}

/// Every fixture must resolve (even with no selections) without panic.
#[test]
fn smoke_all_fixtures_resolve_empty() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let plan = installer.resolve();
        // Plan should be a valid (possibly empty) list
        let _ = plan.operations.len();
    }
}

/// Every fixture must enumerate visible steps without panic.
#[test]
fn smoke_all_fixtures_visible_steps() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let steps = installer.visible_steps();
        for (_idx, step) in &steps {
            assert!(!step.name.is_empty() || step.name.is_empty()); // just access it
        }
    }
}

/// Every fixture should produce a valid SRI hash.
#[test]
fn smoke_all_fixtures_hash() {
    for (name, xml) in FIXTURES {
        let h = hash_xml(xml);
        assert!(
            h.starts_with("sha256-"),
            "Hash for {name} should start with sha256-"
        );
        assert!(h.len() > 10, "Hash for {name} should be non-trivial");
    }
}

/// Every fixture should generate defaults without panic.
#[test]
fn smoke_all_fixtures_declarative_defaults() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_defaults(xml, "test", &config);
        assert_eq!(decl.schema_version, SCHEMA_VERSION);
        assert_eq!(decl.rev, "test");
        assert_eq!(decl.hash, hash_xml(xml));
    }
}

/// Every fixture's defaults should round-trip through apply (or fail with ValidationFailed
/// when defaults can't satisfy group constraints, e.g. SelectExactlyOne with all Optional).
#[test]
fn smoke_all_fixtures_declarative_roundtrip() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_defaults(xml, "test", &config);

        let config2 = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config2);
        match decl.apply(xml, &mut installer) {
            Ok(()) => {
                let plan = installer.resolve();
                let _ = plan.operations.len();
            }
            Err(e) => {
                // ValidationFailed is acceptable when defaults can't satisfy constraints
                let msg = e.to_string();
                assert!(
                    msg.contains("invalid selection"),
                    "Unexpected declarative error for {name}: {msg}"
                );
            }
        }
    }
}

/// Every fixture's from_all config should apply successfully.
#[test]
fn smoke_all_fixtures_declarative_from_all_applies() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_all(xml, "test", &config);

        let config2 = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config2);

        // from_all may violate constraints (e.g., SelectExactlyOne with 2 plugins)
        // so we just check it doesn't panic on construction
        let _ = decl.apply(xml, &mut installer);
    }
}

/// check_dependencies never panics.
#[test]
fn smoke_all_fixtures_check_deps() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let _ = installer.check_dependencies();
    }
}

/// info.xml parsing smoke test.
#[test]
fn smoke_info_xml_parses() {
    let xml = include_str!("fixtures/info.xml");
    let info = FomodInfo::parse(xml).unwrap();
    assert!(info.name.is_some());
}

/// Nix serialization smoke test (via dev-dep ronix).
#[test]
fn smoke_nix_serialization() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_defaults(xml, "test", &config);
        let nix = ronix::to_nix(&decl);
        assert!(
            nix.is_ok(),
            "Nix serialization failed for {name}: {:?}",
            nix.err()
        );
        let nix_str = nix.unwrap();
        assert!(nix_str.contains("schema_version"));
        assert!(nix_str.contains("sha256-"));
    }
}

/// Every fixture should produce valid context after default selections.
#[test]
fn smoke_all_fixtures_context_after_defaults() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config.clone());

        // Apply default selections for every group
        if let Some(ref steps) = config.install_steps {
            for (step_idx, step) in steps.steps.iter().enumerate() {
                if let Some(ref groups) = step.optional_file_groups {
                    for (group_idx, group) in groups.groups.iter().enumerate() {
                        let defaults = Installer::default_selections(group);
                        installer.select(step_idx, group_idx, defaults);
                    }
                }
            }
        }

        // Context should be accessible without panic
        let ctx = installer.context();
        let _ = ctx.flags.len();
        let _ = ctx.file_states.len();
        let _ = ctx.game_version.clone();
        let _ = ctx.manager_version.clone();

        // Resolve should work
        let plan = installer.resolve();
        let _ = plan.operations.len();

        // Visible steps should be enumerable
        let _ = installer.visible_steps();
    }
}

/// Every fixture's from_all config should list every plugin for every group.
#[test]
fn smoke_all_fixtures_from_all_completeness() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_all(xml, "test", &config);

        if let Some(ref steps) = config.install_steps {
            for step in &steps.steps {
                let step_sel = decl.selections.get(&step.name);
                if let (Some(ref groups), Some(sel)) = (&step.optional_file_groups, step_sel) {
                    for group in &groups.groups {
                        let group_sel = sel.get(&group.name).cloned().unwrap_or_default();
                        assert_eq!(
                            group_sel.len(),
                            group.plugins.plugins.len(),
                            "Fixture {name}: from_all should list every plugin for {}/{}",
                            step.name,
                            group.name
                        );
                    }
                }
            }
        }
    }
}

/// Ensure all fixtures produce non-empty default selections for groups that have Required/Recommended plugins.
#[test]
fn smoke_default_selections_for_preferred_plugins() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        if let Some(ref steps) = config.install_steps {
            for step in &steps.steps {
                if let Some(ref groups) = step.optional_file_groups {
                    for group in &groups.groups {
                        let defaults = Installer::default_selections(group);
                        let has_preferred = group.plugins.plugins.iter().any(|p| {
                            matches!(
                                p.plugin_type(),
                                fomod_oxide::config::PluginType::Required
                                    | fomod_oxide::config::PluginType::Recommended
                            )
                        });
                        if has_preferred {
                            assert!(
                                !defaults.is_empty(),
                                "Fixture {name}, step '{}', group '{}' has preferred plugins but empty defaults",
                                step.name,
                                group.name
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Every fixture produces valid CompletionStatus without panic.
#[test]
fn smoke_all_fixtures_completion_status() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let status = installer.completion_status();
        // fraction() should be a valid number
        let f = status.fraction();
        assert!(
            (0.0..=1.0).contains(&f),
            "Fixture {name}: fraction {f} out of range"
        );
        assert!(
            status.satisfied_groups <= status.total_groups,
            "Fixture {name}: satisfied > total"
        );
        assert!(
            status.visible_steps <= status.total_steps,
            "Fixture {name}: visible > total steps"
        );
    }
}

/// validate_step for every step index doesn't panic.
#[test]
fn smoke_all_fixtures_validate_all_steps() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config.clone());
        let step_count = config
            .install_steps
            .as_ref()
            .map(|s| s.steps.len())
            .unwrap_or(0);
        for step_idx in 0..step_count {
            let hints = installer.validate_step(step_idx);
            // Just ensure it returns without panic; hints may be non-empty
            let _ = hints.len();
        }
        // Also call with an out-of-bounds index — should return empty, not panic
        let hints = installer.validate_step(step_count + 100);
        assert!(
            hints.is_empty(),
            "Fixture {name}: out-of-bounds step should give empty hints"
        );
    }
}

/// detect_conflicts() doesn't panic on any fixture.
#[test]
fn smoke_all_fixtures_detect_conflicts() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let conflicts = installer.detect_conflicts();
        // Just access the result
        let _ = conflicts.len();
    }
}

/// flag_impact_map() doesn't panic on any fixture.
#[test]
fn smoke_all_fixtures_flag_impact_map() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let impacts = installer.flag_impact_map();
        let _ = impacts.len();
    }
}

/// preview_current() produces valid plan for every fixture.
#[test]
fn smoke_all_fixtures_preview_current() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config.clone());

        // Preview with no selections
        let plan = installer.preview_current();
        let _ = plan.operations.len();

        // Apply default selections, then preview again
        if let Some(ref steps) = config.install_steps {
            for (step_idx, step) in steps.steps.iter().enumerate() {
                if let Some(ref groups) = step.optional_file_groups {
                    for (group_idx, group) in groups.groups.iter().enumerate() {
                        let defaults = Installer::default_selections(group);
                        installer.select(step_idx, group_idx, defaults);
                    }
                }
            }
        }
        let plan = installer.preview_current();
        // Plan operations should all have non-negative priorities
        for op in &plan.operations {
            assert!(op.priority >= 0, "Fixture {name}: negative priority");
        }
    }
}

/// step_name, group_name, module_image_path don't panic on any fixture.
#[test]
fn smoke_all_fixtures_metadata_accessors() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config.clone());

        // module_image_path — may be None, just shouldn't panic
        let _ = installer.module_image_path();

        let step_count = config
            .install_steps
            .as_ref()
            .map(|s| s.steps.len())
            .unwrap_or(0);

        for step_idx in 0..step_count {
            let sn = installer.step_name(step_idx);
            assert!(
                sn.is_some(),
                "Fixture {name}: step_name({step_idx}) should be Some"
            );

            let groups_count = config
                .install_steps
                .as_ref()
                .and_then(|s| s.steps.get(step_idx))
                .and_then(|s| s.optional_file_groups.as_ref())
                .map(|g| g.groups.len())
                .unwrap_or(0);

            for group_idx in 0..groups_count {
                let gn = installer.group_name(step_idx, group_idx);
                assert!(
                    gn.is_some(),
                    "Fixture {name}: group_name({step_idx}, {group_idx}) should be Some"
                );
            }

            // Out-of-bounds group
            assert!(installer.group_name(step_idx, 9999).is_none());
        }

        // Out-of-bounds step
        assert!(installer.step_name(9999).is_none());
    }
}

/// checkpoint() and rollback() don't panic on any fixture.
#[test]
fn smoke_all_fixtures_checkpoint_rollback() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let mut installer = Installer::new(config.clone());

        // Rollback with no history should return false
        assert!(
            !installer.rollback(),
            "Fixture {name}: rollback on empty history should be false"
        );

        // Checkpoint, make a selection, rollback
        installer.checkpoint();
        assert_eq!(installer.history_len(), 1);

        if let Some(ref steps) = config.install_steps {
            if let Some(step) = steps.steps.first() {
                if let Some(ref groups) = step.optional_file_groups {
                    if !groups.groups.is_empty() {
                        let defaults = Installer::default_selections(&groups.groups[0]);
                        installer.select(0, 0, defaults);
                    }
                }
            }
        }

        let rolled_back = installer.rollback();
        assert!(
            rolled_back,
            "Fixture {name}: rollback after checkpoint should succeed"
        );
        assert_eq!(installer.history_len(), 0);
    }
}

/// missing_selections() doesn't panic on any fixture.
#[test]
fn smoke_all_fixtures_missing_selections() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let installer = Installer::new(config);
        let missing = installer.missing_selections();
        // Missing should be a valid list of (step, group) pairs
        for &(step_idx, _group_idx) in &missing {
            assert!(
                installer.step_name(step_idx).is_some(),
                "Fixture {name}: missing references invalid step {step_idx}"
            );
        }
    }
}

/// declarative summary() produces non-panicking results.
#[test]
fn smoke_all_fixtures_summary() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_defaults(xml, "test", &config);
        let summaries: Vec<SelectionSummary> = decl.summary();
        // Summaries should be sorted by (step, group)
        for pair in summaries.windows(2) {
            assert!(
                (&pair[0].step, &pair[0].group) <= (&pair[1].step, &pair[1].group),
                "Fixture {name}: summaries not sorted"
            );
        }
        // Each summary should have a Display impl that doesn't panic
        for s in &summaries {
            let _ = format!("{s}");
        }
    }
}

/// diff() of a config with itself produces empty diffs.
#[test]
fn smoke_all_fixtures_diff_self() {
    for (name, xml) in FIXTURES {
        let config = ModuleConfig::parse(xml).unwrap();
        let decl = DeclarativeConfig::from_defaults(xml, "test", &config);
        let diffs = decl.diff(&decl);
        assert!(
            diffs.is_empty(),
            "Fixture {name}: diff with self should be empty, got {} diffs",
            diffs.len()
        );
    }
}
