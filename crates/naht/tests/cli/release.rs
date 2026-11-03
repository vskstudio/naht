use naht::commands;

use super::support::child;

#[test]
fn package_plugin_produces_a_loadable_rbxmx_with_the_entry_script() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("Plugin.server.luau"), "-- entry point\nreturn nil").unwrap();
    std::fs::write(src.join("Apply.luau"), "return {}").unwrap();
    std::fs::write(src.join("Client.luau"), "return {}").unwrap();

    let out = dir.path().join("naht-plugin.rbxmx");
    commands::package_plugin(&src, &out).unwrap();

    let dom = rbx_xml::from_reader_default(std::fs::read(&out).unwrap().as_slice()).unwrap();
    let naht = dom
        .root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|i| i.name == "Naht")
        .expect("single top-level Naht folder");
    assert_eq!(naht.class, "Folder");

    let plugin = child(&dom, naht, "Plugin").expect("entry Plugin script under Naht");
    assert_eq!(plugin.class, "Script");
    assert_eq!(
        child(&dom, naht, "Apply").map(|m| m.class.as_str().to_owned()),
        Some("ModuleScript".to_owned())
    );
}

#[test]
fn check_release_version_guards_the_tag_against_the_workspace_version() {
    let version = naht_core::version();
    assert!(commands::check_release_version(&format!("v{version}")).is_ok());
    assert!(commands::check_release_version(version).is_ok());
    assert!(commands::check_release_version("v0.0.0-not-the-version").is_err());
}
