//! Public-boundary checks for the attachment declaration, compiled support and completed offline pose.

use std::collections::BTreeSet;

use tdoll_attachments::{
    Body, BoneName, Character, Coat, DAffine3, DQuat, DVec3, Error, Influence, Length,
    MaterialName, Model, Posed, RestNode, Seconds, SpringChain, Surface, input_digest,
};

const ROOT: usize = 0;
const CHEST: usize = 1;
const SPRING: usize = 2;
const BADGE: usize = 3;
const CHILD: usize = 4;
const OTHER: usize = 5;
const NEAR: usize = 1;
const METAL: usize = 2;
const TRIANGLE: [usize; 3] = [2, 0, 3];
const SHARES: [f64; 3] = [0.5, 0.25, 0.25];

fn bone(text: &str) -> BoneName {
    BoneName::new(text).unwrap()
}

fn material(text: &str) -> MaterialName {
    MaterialName::new(text).unwrap()
}

fn intent() -> Character {
    Character::parse("character {\nattach \"Badge\" to=\"Cloth\"\n}").unwrap()
}

fn dt() -> Seconds {
    Seconds::new(1.0 / 60.0).unwrap()
}

fn no_time() -> Seconds {
    Seconds::new(0.0).unwrap()
}

fn fixture(with_springs: bool) -> Model {
    let root = DAffine3::from_rotation_translation(
        DQuat::from_rotation_y(0.43),
        DVec3::new(0.6, -0.2, 0.4),
    );
    let nodes = vec![
        RestNode {
            name: bone("Root"),
            parent: None,
            local: root,
        },
        RestNode {
            name: bone("Chest"),
            parent: Some(ROOT),
            local: DAffine3::from_translation(DVec3::Y),
        },
        RestNode {
            name: bone("Spring"),
            parent: Some(CHEST),
            local: DAffine3::from_translation(DVec3::new(0.2, 0.5, 0.0)),
        },
        RestNode {
            name: bone("Badge"),
            parent: Some(ROOT),
            local: DAffine3::from_rotation_translation(
                DQuat::from_rotation_z(0.22) * DQuat::from_rotation_x(0.18),
                DVec3::new(0.2, 1.2, 0.12),
            ),
        },
        RestNode {
            name: bone("Pin"),
            parent: Some(BADGE),
            local: DAffine3::from_translation(DVec3::new(0.03, 0.01, 0.04)),
        },
        RestNode {
            name: bone("Other"),
            parent: Some(ROOT),
            local: DAffine3::from_translation(DVec3::new(0.5, 1.3, 0.1)),
        },
    ];
    let mut model = Model {
        nodes,
        surfaces: Vec::new(),
        chains: Vec::new(),
        body_bones: BTreeSet::from([bone("Root"), bone("Chest")]),
    };
    let worlds = model.worlds(&model.rest_locals()).unwrap();
    let near = Surface {
        material: material("Cloth"),
        joints: vec![bone("Spring"), bone("Root"), bone("Chest")],
        binds: vec![
            worlds[SPRING].inverse(),
            worlds[ROOT].inverse(),
            worlds[CHEST].inverse(),
        ],
        held: [
            DVec3::new(0.8, 1.0, 0.0),
            DVec3::new(8.0, 9.0, 7.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(0.0, 1.8, 0.0),
        ]
        .map(|point| root.transform_point3(point))
        .to_vec(),
        weights: vec![
            vec![
                Influence {
                    joint: 2,
                    weight: 0.3,
                },
                Influence {
                    joint: 0,
                    weight: 0.7,
                },
            ],
            vec![Influence {
                joint: 1,
                weight: 1.0,
            }],
            vec![Influence {
                joint: 2,
                weight: 1.0,
            }],
            vec![Influence {
                joint: 0,
                weight: 1.0,
            }],
        ],
        triangles: vec![TRIANGLE],
    };
    let mut far = near.clone();
    for point in &mut far.held {
        *point += root.transform_vector3(DVec3::new(0.0, 0.0, -4.0));
    }
    let piece = Surface {
        material: material("Metal"),
        joints: vec![bone("Badge")],
        binds: vec![worlds[BADGE].inverse()],
        held: [
            DVec3::new(-0.06, -0.04, 0.0),
            DVec3::new(0.06, -0.04, 0.0),
            DVec3::new(0.0, 0.08, 0.0),
        ]
        .map(|point| worlds[BADGE].transform_point3(point))
        .to_vec(),
        weights: vec![
            vec![Influence {
                joint: 0,
                weight: 1.0
            }];
            3
        ],
        triangles: vec![[0, 1, 2]],
    };
    model.surfaces = vec![far, near, piece];
    if with_springs {
        model.chains.push(SpringChain {
            bones: vec![bone("Spring")],
            tail: Length::new(0.4).unwrap(),
            stiffness: 0.3,
            drag: 0.2,
            gravity: 1.0,
        });
    }
    model
}

fn named(model: &Model, worlds: &[DAffine3]) -> Vec<(BoneName, DAffine3)> {
    model
        .nodes
        .iter()
        .zip(worlds)
        .map(|(node, world)| (node.name.clone(), *world))
        .collect()
}

fn frame(points: &[DVec3], triangle: [usize; 3], shares: [f64; 3]) -> DAffine3 {
    let [a, b, c] = triangle.map(|index| points[index]);
    let x = (b - a).normalize();
    let y = (b - a).cross(c - a).normalize();
    let origin = a * shares[0] + b * shares[1] + c * shares[2];
    DAffine3::from_mat3_translation(glam::DMat3::from_cols(x, y, x.cross(y)), origin)
}

fn near_transform(actual: DAffine3, expected: DAffine3) {
    assert!(
        actual.abs_diff_eq(expected, 1e-8),
        "actual {actual:?}, expected {expected:?}"
    );
}

fn assert_followed(model: &Model, posed: &Posed, shares: [f64; 3]) {
    let rest = model.worlds(&model.rest_locals()).unwrap();
    let rest_surface = model.surfaces[NEAR].posed(&named(model, &rest));
    let rest_frame = frame(&rest_surface, TRIANGLE, shares);
    let current_frame = frame(&posed.surfaces[NEAR], TRIANGLE, shares);
    let expected = current_frame * rest_frame.inverse() * rest[BADGE];
    near_transform(posed.worlds[BADGE], expected);
    near_transform(posed.worlds[CHILD], expected * model.nodes[CHILD].local);
    for (index, vertex) in posed.surfaces[METAL].iter().enumerate() {
        let held = model.surfaces[METAL].held[index];
        let expected_vertex = (expected * rest[BADGE].inverse()).transform_point3(held);
        assert!(
            vertex.abs_diff_eq(expected_vertex, 1e-8),
            "vertex {index}: {vertex:?}"
        );
    }
    for a in 0..3 {
        for b in a + 1..3 {
            let held = model.surfaces[METAL].held[a].distance(model.surfaces[METAL].held[b]);
            let drawn = posed.surfaces[METAL][a].distance(posed.surfaces[METAL][b]);
            assert!(
                (held - drawn).abs() <= 1e-6,
                "piece stretched: {held} to {drawn}"
            );
        }
    }
}

fn moved_locals(model: &Model) -> Vec<DAffine3> {
    let mut locals = model.rest_locals();
    locals[ROOT] = DAffine3::from_rotation_translation(
        DQuat::from_rotation_y(-0.27) * DQuat::from_rotation_z(0.2),
        DVec3::new(-0.4, 0.7, 1.1),
    );
    locals[CHEST] *= DAffine3::from_rotation_translation(
        DQuat::from_rotation_x(0.56),
        DVec3::new(0.1, 0.0, 0.2),
    );
    locals[SPRING] *= DAffine3::from_rotation_z(-0.64);
    locals
}

#[test]
fn authoring_retains_bone_and_material_names_and_refuses_bad_statements() {
    let character = intent();
    assert_eq!(character.attachments.len(), 1);
    assert_eq!(character.attachments[0].piece, bone("Badge"));
    assert_eq!(character.attachments[0].surface, material("Cloth"));
    assert!(matches!(
        Character::parse("attach Badge to=\"Cloth\""),
        Err(Error::Declaration { .. })
    ));
}

#[test]
fn rest_pose_retains_authored_piece_position_orientation_and_all_vertices() {
    let model = fixture(false);
    let body = Body::build(model.clone(), &intent()).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let posed = body
        .frame(&model.rest_locals(), &mut coat, no_time())
        .unwrap();
    let rest = model.worlds(&model.rest_locals()).unwrap();
    near_transform(posed.worlds[BADGE], rest[BADGE]);
    for (drawn, held) in posed.surfaces[METAL]
        .iter()
        .zip(&model.surfaces[METAL].held)
    {
        assert!(drawn.abs_diff_eq(*held, 1e-9));
    }
    assert_followed(&model, &posed, SHARES);
}

#[test]
fn support_uses_the_nearest_cut_primitive_and_records_barycentric_rest_frame() {
    let model = fixture(false);
    let body = Body::build(model.clone(), &intent()).unwrap();
    let [attachment] = body.attachments() else {
        panic!("one compiled attachment")
    };
    assert_eq!(attachment.piece, bone("Badge"));
    assert_eq!(attachment.surface, material("Cloth"));
    assert_eq!(attachment.primitive, NEAR);
    assert_eq!(attachment.triangle, TRIANGLE);
    for (actual, expected) in attachment.shares.iter().zip(SHARES) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "shares {:?}",
            attachment.shares
        );
    }
    let rest = model.worlds(&model.rest_locals()).unwrap();
    let surface = model.surfaces[NEAR].posed(&named(&model, &rest));
    let support = frame(&surface, TRIANGLE, SHARES);
    near_transform(attachment.rest_frame, support);
    near_transform(attachment.offset, support.inverse() * rest[BADGE]);
}

#[test]
fn skeleton_alone_translates_rotates_and_shears_support_without_stretching_piece() {
    let model = fixture(false);
    let body = Body::build(model.clone(), &intent()).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let posed = body.frame(&moved_locals(&model), &mut coat, dt()).unwrap();
    assert_followed(&model, &posed, SHARES);
    let rest = model.worlds(&model.rest_locals()).unwrap();
    assert!(!posed.worlds[BADGE].abs_diff_eq(rest[BADGE], 1e-3));
    let before = model.surfaces[NEAR].held[0].distance(model.surfaces[NEAR].held[3]);
    let after = posed.surfaces[NEAR][0].distance(posed.surfaces[NEAR][3]);
    assert!(
        (before - after).abs() > 1e-3,
        "fixture must genuinely deform the support"
    );
}

#[test]
fn spring_solver_runs_before_follower_even_when_the_clip_is_still() {
    let model = fixture(true);
    let body = Body::build(model.clone(), &intent()).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let locals = model.rest_locals();
    let mut posed = body.frame(&locals, &mut coat, dt()).unwrap();
    for _ in 0..120 {
        posed = body.frame(&locals, &mut coat, dt()).unwrap();
    }
    let rest = model.worlds(&locals).unwrap();
    assert!(
        !posed.worlds[SPRING].abs_diff_eq(rest[SPRING], 1e-4),
        "spring must really move"
    );
    assert_followed(&model, &posed, SHARES);
    assert!(!posed.worlds[BADGE].abs_diff_eq(rest[BADGE], 1e-4));
}

#[test]
fn skeletal_animation_and_springs_both_feed_the_completed_surface_frame() {
    let model = fixture(true);
    let body = Body::build(model.clone(), &intent()).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let locals = moved_locals(&model);
    let skeletal = model.worlds(&locals).unwrap();
    let mut posed = body.frame(&locals, &mut coat, dt()).unwrap();
    for _ in 0..120 {
        posed = body.frame(&locals, &mut coat, dt()).unwrap();
    }
    assert!(!posed.worlds[SPRING].abs_diff_eq(skeletal[SPRING], 1e-4));
    assert_followed(&model, &posed, SHARES);
}

fn move_piece(model: &mut Model, centre: DVec3) {
    let old_world = model.worlds(&model.rest_locals()).unwrap()[BADGE];
    model.nodes[BADGE].local.translation = centre;
    let new_world = model.worlds(&model.rest_locals()).unwrap()[BADGE];
    for point in &mut model.surfaces[METAL].held {
        *point = (new_world * old_world.inverse()).transform_point3(*point);
    }
    model.surfaces[METAL].binds[0] = new_world.inverse();
}

#[test]
fn closest_point_clamps_to_triangle_edges_and_vertices() {
    for (centre, shares) in [
        (DVec3::new(0.4, 0.8, 0.12), [0.5, 0.5, 0.0]),
        (DVec3::new(-0.2, 0.8, 0.12), [1.0, 0.0, 0.0]),
    ] {
        let mut model = fixture(false);
        move_piece(&mut model, centre);
        let body = Body::build(model.clone(), &intent()).unwrap();
        for (actual, expected) in body.attachments()[0].shares.iter().zip(shares) {
            assert!(
                (actual - expected).abs() < 1e-9,
                "shares {:?}",
                body.attachments()[0].shares
            );
        }
        let mut coat = Coat::new(&model).unwrap();
        let posed = body.frame(&moved_locals(&model), &mut coat, dt()).unwrap();
        assert_followed(&model, &posed, shares);
    }
}

#[test]
fn degenerate_nearest_triangle_is_skipped_without_losing_valid_support() {
    let mut model = fixture(false);
    let mut degenerate = model.surfaces[NEAR].clone();
    degenerate.triangles = vec![[2, 2, 0]];
    model.surfaces.insert(0, degenerate);
    let body = Body::build(model, &intent()).unwrap();
    assert_eq!(body.attachments()[0].primitive, NEAR + 1);
    assert_eq!(body.attachments()[0].triangle, TRIANGLE);
}

#[test]
fn collinear_or_zero_area_support_is_a_typed_named_refusal() {
    for collinear in [false, true] {
        let mut model = fixture(false);
        for primitive in [0, NEAR] {
            let surface = &mut model.surfaces[primitive];
            if collinear {
                surface.held[3] = surface.held[2] + (surface.held[0] - surface.held[2]) * 0.4;
            } else {
                surface.triangles = vec![[2, 2, 0]];
            }
        }
        let error = Body::build(model, &intent()).unwrap_err();
        assert!(
            matches!(error, Error::NoSupport { ref piece, ref surface }
            if piece == &bone("Badge") && surface == &material("Cloth")),
            "{error:?}"
        );
        assert!(error.to_string().contains("Badge") && error.to_string().contains("Cloth"));
    }
}

#[test]
fn unknown_piece_missing_material_self_attachment_and_duplicate_are_named_refusals() {
    let cases = [
        ("attach \"Ghost\" to=\"Cloth\"", "unknown"),
        ("attach \"Badge\" to=\"Absent\"", "missing"),
        ("attach \"Badge\" to=\"Metal\"", "self"),
        (
            "attach \"Badge\" to=\"Cloth\"\nattach \"Badge\" to=\"Cloth\"",
            "duplicate",
        ),
    ];
    for (text, kind) in cases {
        let character = Character::parse(text).unwrap();
        let error = Body::build(fixture(false), &character).unwrap_err();
        let correct = match &error {
            Error::UnknownPiece { piece } => kind == "unknown" && piece == &bone("Ghost"),
            Error::MissingMaterial { piece, surface } => {
                kind == "missing" && piece == &bone("Badge") && surface == &material("Absent")
            }
            Error::SelfAttachment { piece, surface } => {
                kind == "self" && piece == &bone("Badge") && surface == &material("Metal")
            }
            Error::Duplicate { piece } => kind == "duplicate" && piece == &bone("Badge"),
            _ => false,
        };
        assert!(correct, "{kind}: {error:?}");
    }
}

#[test]
fn body_and_spring_bone_ownership_cannot_be_claimed_by_an_attachment() {
    for spring_owner in [false, true] {
        let mut model = fixture(false);
        if spring_owner {
            model.chains.push(SpringChain {
                bones: vec![bone("Badge")],
                tail: Length::new(0.1).unwrap(),
                stiffness: 0.3,
                drag: 0.2,
                gravity: 0.0,
            });
        } else {
            model.body_bones.insert(bone("Badge"));
        }
        let error = Body::build(model, &intent()).unwrap_err();
        assert!(
            matches!(error, Error::Ownership { ref piece, ref bone }
            if piece.as_str() == "Badge" && bone.as_str() == "Badge"),
            "{error:?}"
        );
    }
}

#[test]
fn mixed_skin_weights_cannot_make_a_rigid_piece_share_a_body_bone() {
    let mut model = fixture(false);
    let rest = model.worlds(&model.rest_locals()).unwrap();
    let piece = &mut model.surfaces[METAL];
    piece.joints.push(bone("Chest"));
    piece.binds.push(rest[CHEST].inverse());
    piece.weights[0] = vec![
        Influence {
            joint: 0,
            weight: 0.6,
        },
        Influence {
            joint: 1,
            weight: 0.4,
        },
    ];
    let error = Body::build(model, &intent()).unwrap_err();
    assert!(
        matches!(error, Error::Ownership { ref piece, .. } if piece.as_str() == "Badge"),
        "{error:?}"
    );
    assert!(error.to_string().contains("Badge"));
}

#[test]
fn two_attachments_cannot_share_a_triangle_or_its_bones() {
    let mut model = fixture(false);
    let rest = model.worlds(&model.rest_locals()).unwrap();
    let piece = &mut model.surfaces[METAL];
    piece.joints.push(bone("Other"));
    piece.binds.push(rest[OTHER].inverse());
    for weights in &mut piece.weights {
        *weights = vec![
            Influence {
                joint: 0,
                weight: 0.5,
            },
            Influence {
                joint: 1,
                weight: 0.5,
            },
        ];
    }
    let character =
        Character::parse("attach \"Badge\" to=\"Cloth\"\nattach \"Other\" to=\"Cloth\"").unwrap();
    let error = Body::build(model, &character).unwrap_err();
    assert!(
        matches!(error, Error::Ownership { ref piece, .. }
        if piece.as_str() == "Badge" || piece.as_str() == "Other"),
        "{error:?}"
    );
}

#[test]
fn collapsed_posed_support_keeps_the_piece_skeletal_pose_and_stays_finite() {
    let mut model = fixture(false);
    for primitive in [0, NEAR] {
        for weights in &mut model.surfaces[primitive].weights {
            *weights = vec![Influence {
                joint: 2,
                weight: 1.0,
            }];
        }
    }
    let body = Body::build(model.clone(), &intent()).unwrap();
    let mut locals = model.rest_locals();
    locals[CHEST] *= DAffine3::from_scale(DVec3::new(0.0, 1.0, 1.0));
    let skeletal = model.worlds(&locals).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let posed = body.frame(&locals, &mut coat, dt()).unwrap();
    assert!(posed.worlds.iter().all(|world| world.is_finite()));
    near_transform(posed.worlds[BADGE], skeletal[BADGE]);
}

#[test]
fn changing_only_attachment_declarations_invalidates_derived_cache_inputs() {
    let mut model = fixture(false);
    let mut lining = model.surfaces[NEAR].clone();
    lining.material = material("Lining");
    model.surfaces.push(lining);
    let rest = model.worlds(&model.rest_locals()).unwrap();
    let mut other = model.surfaces[METAL].clone();
    other.material = material("SpareMetal");
    other.joints = vec![bone("Other")];
    other.binds = vec![rest[OTHER].inverse()];
    for held in &mut other.held {
        *held = (rest[OTHER] * rest[BADGE].inverse()).transform_point3(*held);
    }
    model.surfaces.push(other);
    let plain = input_digest(&model, &Character::default()).unwrap();
    let attached = input_digest(&model, &intent()).unwrap();
    assert_ne!(
        plain, attached,
        "model bytes alone do not describe the offline pose"
    );
    let mut changed_material = intent();
    changed_material.attachments[0].surface = material("Lining");
    assert_ne!(attached, input_digest(&model, &changed_material).unwrap());
    let mut changed_piece = intent();
    changed_piece.attachments[0].piece = bone("Other");
    assert_ne!(attached, input_digest(&model, &changed_piece).unwrap());
    assert_eq!(
        attached,
        input_digest(&model, &intent()).unwrap(),
        "digest must be deterministic"
    );
}

#[test]
fn no_attachment_behaviour_keeps_the_existing_skin_and_spring_pose() {
    let model = fixture(false);
    let body = Body::build(model.clone(), &Character::default()).unwrap();
    assert!(body.attachments().is_empty());
    let locals = moved_locals(&model);
    let expected = model.worlds(&locals).unwrap();
    let mut coat = Coat::new(&model).unwrap();
    let posed = body.frame(&locals, &mut coat, dt()).unwrap();
    for (actual, expected) in posed.worlds.iter().zip(&expected) {
        near_transform(*actual, *expected);
    }
    let worlds = named(&model, &expected);
    for (surface, actual) in model.surfaces.iter().zip(&posed.surfaces) {
        for (actual, expected) in actual.iter().zip(surface.posed(&worlds)) {
            assert!(actual.abs_diff_eq(expected, 1e-9));
        }
    }
}
