use tdoll_attachments::{BoneName, Character, MaterialName};

#[test]
fn character_boundary_retains_names() {
    let declaration = Character::parse("character {\nattach \"Badge\" to=\"Cloth\"\n}").unwrap();
    assert_eq!(
        declaration.attachments[0].piece,
        BoneName::new("Badge").unwrap()
    );
    assert_eq!(
        declaration.attachments[0].surface,
        MaterialName::new("Cloth").unwrap()
    );
}

#[test]
fn character_boundary_refuses_unquoted_names() {
    assert!(Character::parse("attach Badge to=\"Cloth\"").is_err());
    assert!(MaterialName::new("").is_err());
}
