use glam::DAffine3;
use serde::{Deserialize, Serialize};
use tdoll_vocab::BoneName;

use crate::{Character, MaterialName, Model};

/// Durable support in the published, cut model. Shares locate the closest point to the rest centroid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub piece: BoneName,
    pub surface: MaterialName,
    pub primitive: usize,
    pub triangle: [usize; 3],
    pub shares: [f64; 3],
    pub rest_frame: DAffine3,
    pub offset: DAffine3,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid character declaration on line {line}")]
    Declaration { line: usize },
    #[error("invalid cut mesh, skin or parent-first skeleton")]
    Model,
    #[error("attachment piece {piece} is not implemented")]
    Unsupported { piece: BoneName },
    #[error("unknown attachment piece {piece}")]
    UnknownPiece { piece: BoneName },
    #[error("piece {piece} names missing material {surface}")]
    MissingMaterial {
        piece: BoneName,
        surface: MaterialName,
    },
    #[error("piece {piece} cannot attach to itself on material {surface}")]
    SelfAttachment {
        piece: BoneName,
        surface: MaterialName,
    },
    #[error("piece {piece} has conflicting ownership of bone {bone}")]
    Ownership { piece: BoneName, bone: BoneName },
    #[error("piece {piece} is attached more than once")]
    Duplicate { piece: BoneName },
    #[error("piece {piece} has no non-degenerate support on material {surface}")]
    NoSupport {
        piece: BoneName,
        surface: MaterialName,
    },
    #[error(transparent)]
    Springs(#[from] tdoll_springs::Error),
}

pub(crate) fn bind(_model: &Model, character: &Character) -> Result<Vec<Attachment>, Error> {
    if let Some(declaration) = character.attachments.first() {
        return Err(Error::Unsupported {
            piece: declaration.piece.clone(),
        });
    }
    Ok(Vec::new())
}

pub(crate) fn follow(
    _model: &Model,
    _attachments: &[Attachment],
    _worlds: &mut [DAffine3],
) -> Result<(), Error> {
    Ok(())
}
