use std::collections::BTreeSet;

use glam::{DAffine3, DVec3};
use serde::{Deserialize, Serialize};
use tdoll_vocab::BoneName;

use crate::{Error, MaterialName};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestNode {
    pub name: BoneName,
    pub parent: Option<usize>,
    pub local: DAffine3,
}

/// A positive joint weight, indexed in this surface's skin, not in the skeleton.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Influence {
    pub joint: usize,
    pub weight: f64,
}

/// A published primitive after cutting; indices are local to this primitive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Surface {
    pub material: MaterialName,
    pub joints: Vec<BoneName>,
    pub binds: Vec<DAffine3>,
    pub held: Vec<DVec3>,
    pub weights: Vec<Vec<Influence>>,
    pub triangles: Vec<[usize; 3]>,
}

impl Surface {
    /// glTF linear blend skinning, retaining the model's own inverse binds.
    pub fn posed(&self, worlds: &[(BoneName, DAffine3)]) -> Vec<DVec3> {
        self.held
            .iter()
            .zip(&self.weights)
            .map(|(held, weights)| {
                weights
                    .iter()
                    .filter_map(|weight| {
                        let name = self.joints.get(weight.joint)?;
                        let world = worlds.iter().find(|(bone, _)| bone == name)?.1;
                        let bind = self.binds.get(weight.joint)?;
                        Some((world * *bind).transform_point3(*held) * weight.weight)
                    })
                    .sum()
            })
            .collect()
    }
}

/// The focused host exposes the same chain feel to the genuine XPBD solver, without furniture or joint boxes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpringChain {
    pub bones: Vec<BoneName>,
    pub tail: tdoll_vocab::Length,
    pub stiffness: f64,
    pub drag: f64,
    pub gravity: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub nodes: Vec<RestNode>,
    pub surfaces: Vec<Surface>,
    pub chains: Vec<SpringChain>,
    pub body_bones: BTreeSet<BoneName>,
}

impl Model {
    pub fn rest_locals(&self) -> Vec<DAffine3> {
        self.nodes.iter().map(|node| node.local).collect()
    }

    /// Parent-first skeleton walk, as the offline coat uses before and after solving.
    pub fn worlds(&self, locals: &[DAffine3]) -> Result<Vec<DAffine3>, Error> {
        if locals.len() != self.nodes.len() {
            return Err(Error::Model);
        }
        let mut worlds = Vec::with_capacity(locals.len());
        for (index, (node, local)) in self.nodes.iter().zip(locals).enumerate() {
            let parent = match node.parent {
                Some(parent) if parent < index => *worlds.get(parent).ok_or(Error::Model)?,
                Some(_) => return Err(Error::Model),
                None => DAffine3::IDENTITY,
            };
            worlds.push(parent * *local);
        }
        Ok(worlds)
    }

    pub(crate) fn named(&self, worlds: &[DAffine3]) -> Vec<(BoneName, DAffine3)> {
        self.nodes
            .iter()
            .zip(worlds)
            .map(|(node, world)| (node.name.clone(), *world))
            .collect()
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        self.worlds(&self.rest_locals())?;
        let names: BTreeSet<_> = self.nodes.iter().map(|node| &node.name).collect();
        if names.len() != self.nodes.len() {
            return Err(Error::Model);
        }
        for surface in &self.surfaces {
            if surface.joints.len() != surface.binds.len()
                || surface.held.len() != surface.weights.len()
                || surface.joints.iter().any(|name| !names.contains(name))
                || surface.held.iter().any(|point| !point.is_finite())
                || surface.binds.iter().any(|bind| !bind.is_finite())
                || surface
                    .triangles
                    .iter()
                    .flatten()
                    .any(|&index| index >= surface.held.len())
            {
                return Err(Error::Model);
            }
            for weights in &surface.weights {
                let sum: f64 = weights.iter().map(|weight| weight.weight).sum();
                if (sum - 1.0).abs() > 1e-9
                    || !sum.is_finite()
                    || weights
                        .iter()
                        .any(|weight| weight.joint >= surface.joints.len() || weight.weight < 0.0)
                {
                    return Err(Error::Model);
                }
            }
        }
        Ok(())
    }
}
