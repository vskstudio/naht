//! Golden fixtures pin the encoded bytes of a small place and model, so an upgrade that changes
//! the output of the encoder fails here instead of silently reshaping every build.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use naht_core::Snapshot;
use naht_core::build::{ModelFormat, write_model, write_place};
use rbx_dom_weak::Instance;
use rbx_dom_weak::types::{CFrame, Content, ContentId, Matrix3, Variant, Vector3};

type TestResult = Result<(), Box<dyn Error>>;

fn scene() -> Snapshot {
    let tilted = Matrix3 {
        x: Vector3::new(0.866_025_4, 0.0, -0.5),
        y: Vector3::new(0.0, 1.0, 0.0),
        z: Vector3::new(0.5, 0.0, 0.866_025_4),
    };
    let crate_part = Snapshot::new("Part", "Crate")
        .with_property(
            "CFrame",
            Variant::CFrame(CFrame::new(Vector3::new(1.5, 2.0, -3.0), tilted)),
        )
        .with_child(Snapshot::new("Decal", "Face").with_property(
            "Texture",
            Variant::Content(Content::from_uri("rbxassetid://1818")),
        ));
    let hum = Snapshot::new("Sound", "Hum").with_property(
        "SoundId",
        Variant::ContentId(ContentId::from("rbxassetid://9".to_owned())),
    );
    let workspace = Snapshot::new("Folder", "Workspace")
        .with_child(crate_part)
        .with_child(hum);
    Snapshot::new("Folder", "proj").with_child(workspace)
}

fn golden(name: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    Ok(fs::read(path)?)
}

fn encoded_place() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    write_place(&mut bytes, &scene(), ModelFormat::Binary)?;
    Ok(bytes)
}

fn encoded_model() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    write_model(&mut bytes, &scene(), ModelFormat::Binary)?;
    Ok(bytes)
}

fn find_instance<'a>(dom: &'a rbx_dom_weak::WeakDom, name: &str) -> Option<&'a Instance> {
    dom.descendants().find(|instance| instance.name == name)
}

fn cframe_of(instance: &Instance) -> Option<&CFrame> {
    instance.properties.values().find_map(|value| {
        if let Variant::CFrame(cframe) = value {
            Some(cframe)
        } else {
            None
        }
    })
}

fn has_value(instance: &Instance, expected: &Variant) -> bool {
    instance.properties.values().any(|value| value == expected)
}

#[test]
fn place_bytes_match_the_golden_fixture() -> TestResult {
    assert_eq!(encoded_place()?, golden("golden_place.rbxl")?);
    Ok(())
}

#[test]
fn model_bytes_match_the_golden_fixture() -> TestResult {
    assert_eq!(encoded_model()?, golden("golden_model.rbxm")?);
    Ok(())
}

#[test]
fn golden_place_keeps_the_tilted_cframe_and_the_decal_texture() -> TestResult {
    let dom = rbx_binary::from_reader(golden("golden_place.rbxl")?.as_slice())?;

    let crate_part = find_instance(&dom, "Crate").ok_or("Crate missing")?;
    let cframe = cframe_of(crate_part).ok_or("CFrame missing")?;
    assert_eq!(cframe.orientation.z, Vector3::new(0.5, 0.0, 0.866_025_4));

    let decal = find_instance(&dom, "Face").ok_or("Face missing")?;
    assert!(has_value(
        decal,
        &Variant::Content(Content::from_uri("rbxassetid://1818"))
    ));
    Ok(())
}
