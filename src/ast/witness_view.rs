//! Source argument roles for bounded witness views. The shared owner vocabulary
//! does not establish source provenance, lifetime or a machine read bound.
use super::{Type, TypeArgument};
use cellscript_artifact_checker::view_types::{WitnessBytesOwner, WitnessBytesViewType};

pub(crate) const MAX_WITNESS_BYTES_VIEW: u64 = 65_536;

impl Type {
    pub fn witness_bytes_view(owner: WitnessBytesOwner, maximum: u64) -> Self {
        Self::applied(
            "WitnessBytesView",
            vec![TypeArgument::Type(Self::Named(owner.as_str().into())), TypeArgument::UnsignedConstant(maximum.into())],
        )
    }

    pub fn as_witness_bytes_view(&self) -> Option<WitnessBytesViewType> {
        let Self::Applied(application) = self else { return None };
        if application.constructor != "WitnessBytesView" {
            return None;
        }
        let [TypeArgument::Type(Self::Named(owner)), TypeArgument::UnsignedConstant(maximum)] = application.arguments.as_slice()
        else {
            return None;
        };
        Some(WitnessBytesViewType { owner: WitnessBytesOwner::parse(owner)?, maximum: u64::try_from(*maximum).ok()? })
    }
}
