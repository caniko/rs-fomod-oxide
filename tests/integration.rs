use fomod_oxide::{FomodInfo, Installer, ModuleConfig};
use fomod_oxide::condition::EvalContext;
use fomod_oxide::config::{GroupType, PluginType};

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
    assert_eq!(groups[0].plugins.plugins[0].plugin_type(), PluginType::Recommended);
    assert_eq!(groups[0].plugins.plugins[1].plugin_type(), PluginType::Optional);

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
        .optional_file_groups.as_ref().unwrap().groups;

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
    assert!(plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));

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
    assert!(!plan.operations.iter().any(|op| op.source == "patches/hd_normals.esp"));
}

#[test]
fn validate_selection_constraints() {
    let config = ModuleConfig::parse(CONFIG_XML).unwrap();
    let group = &config.install_steps.as_ref().unwrap().steps[0]
        .optional_file_groups.as_ref().unwrap().groups[0];

    // SelectExactlyOne: must select 1
    assert!(Installer::validate_selection(group, &[0]).is_ok());
    assert!(Installer::validate_selection(group, &[]).is_err());
    assert!(Installer::validate_selection(group, &[0, 1]).is_err());
    assert!(Installer::validate_selection(group, &[99]).is_err());
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
