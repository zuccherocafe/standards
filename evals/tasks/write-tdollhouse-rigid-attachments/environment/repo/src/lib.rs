//! The cut model, the declaration and the offline pose used by conversion and derived geometry caches.

mod attachments;
mod character;
mod coat;
mod mesh;

pub use attachments::{Attachment, Error};
pub use character::{AttachmentDecl, Character, MaterialName};
pub use coat::Coat;
pub use glam::{DAffine3, DQuat, DVec3};
pub use mesh::{Influence, Model, RestNode, SpringChain, Surface};
pub use tdoll_vocab::{BoneName, Length, Seconds};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// A built body keeps the same model for support discovery, spring solving and final skinning.
#[derive(Clone, Debug)]
pub struct Body {
    model: Model,
    attachments: Vec<Attachment>,
}

/// A completed offline pose, with its final bone worlds and one vertex array per cut surface.
#[derive(Clone, Debug)]
pub struct Posed {
    pub worlds: Vec<DAffine3>,
    pub surfaces: Vec<Vec<DVec3>>,
}

impl Body {
    /// Bind character intent against the published, already cut mesh.
    pub fn build(model: Model, character: &Character) -> Result<Self, Error> {
        model.validate()?;
        let attachments = attachments::bind(&model, character)?;
        Ok(Self { model, attachments })
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    /// Skeleton first, spring solvers second, rigid followers last, then skin every mesh vertex.
    pub fn frame(
        &self,
        locals: &[DAffine3],
        coat: &mut Coat,
        delta: Seconds,
    ) -> Result<Posed, Error> {
        let mut worlds = coat.frame(&self.model, locals, delta)?;
        attachments::follow(&self.model, &self.attachments, &mut worlds)?;
        let named = self.model.named(&worlds);
        let surfaces = self
            .model
            .surfaces
            .iter()
            .map(|surface| surface.posed(&named))
            .collect();
        Ok(Posed { worlds, surfaces })
    }
}

/// A versioned input envelope for caches derived from a posed body.
#[derive(Serialize)]
struct CacheInputs<'a> {
    version: u32,
    model: &'a Model,
}

/// Hash every model input used by derived offline geometry.
pub fn input_digest(model: &Model, _character: &Character) -> Result<[u8; 32], serde_json::Error> {
    let bytes = serde_json::to_vec(&CacheInputs { version: 3, model })?;
    Ok(Sha256::digest(bytes).into())
}
