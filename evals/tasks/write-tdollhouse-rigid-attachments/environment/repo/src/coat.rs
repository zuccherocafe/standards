use glam::{DAffine3, DQuat};
use tdoll_springs::chain::ChainSpec;
use tdoll_springs::furniture::Furniture;
use tdoll_springs::joint::{Family, Feel};
use tdoll_springs::pose::Pose;
use tdoll_springs::{Bone, BoneIndex, Frame, Poses, Springs, Unmeasured};
use tdoll_vocab::{Length, Seconds};

use crate::{Error, Model};

/// The real engine-independent solver; its host writes solved local rotations, then walks the skeleton again.
#[derive(Debug)]
pub struct Coat {
    springs: Springs,
}

struct Taken<'a> {
    locals: &'a [DAffine3],
    worlds: &'a [DAffine3],
}

impl Poses for Taken<'_> {
    fn local(&self, bone: BoneIndex) -> Pose {
        let (scale, rotation, translation) =
            self.locals[bone.index()].to_scale_rotation_translation();
        Pose {
            scale,
            rotation,
            translation,
        }
    }

    fn world(&self, bone: Option<BoneIndex>) -> DAffine3 {
        bone.map_or(DAffine3::IDENTITY, |bone| self.worlds[bone.index()])
    }
}

impl Coat {
    pub fn new(model: &Model) -> Result<Self, Error> {
        model.validate()?;
        let bones = model
            .nodes
            .iter()
            .map(|node| Bone {
                name: Some(node.name.clone()),
                parent: node.parent,
                rest: node.local,
            })
            .collect();
        let mut springs = Springs::new(bones, tdoll_springs::RATE);
        for chain in &model.chains {
            let family = Family {
                gravity: Some(chain.gravity),
                ..Family::default()
            };
            springs.add_chain(&ChainSpec {
                bones: chain.bones.clone(),
                tail: chain.tail,
                feel: Feel::played(chain.stiffness, chain.drag, Length::ZERO, &family),
                limits: Vec::new(),
                colliders: Vec::new(),
            })?;
        }
        Ok(Self { springs })
    }

    pub(crate) fn frame(
        &mut self,
        model: &Model,
        locals: &[DAffine3],
        delta: Seconds,
    ) -> Result<Vec<DAffine3>, Error> {
        let worlds = model.worlds(locals)?;
        let taken = Taken {
            locals,
            worlds: &worlds,
        };
        if let Frame::Step { .. } = self.springs.capture(delta, &taken) {
            self.springs.advance(&Furniture::default(), &mut Unmeasured);
        }
        let mut drawn = locals.to_vec();
        for (bone, rotation) in self.springs.drawn() {
            let local = &mut drawn[bone.index()];
            let (scale, _, translation) = local.to_scale_rotation_translation();
            *local = DAffine3::from_scale_rotation_translation(
                scale,
                DQuat::normalize(rotation),
                translation,
            );
        }
        model.worlds(&drawn)
    }
}
