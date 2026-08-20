//! x86_64 TLB invalidation policy boundary.
//!
//! Page-table mutation and TLB invalidation are separate mechanisms. This
//! module owns the architecture-specific choice so higher layers never need
//! to sprinkle INVLPG/INVPCID directly through the kernel.

use x86_64::instructions::tlb::{self, InvPcidCommand, Pcid};
use x86_64::VirtAddr;

use super::paging::CpuFeatures;
use super::pcid::AddressSpacePcid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TlbInvalidationError {
    PcidUnsupported,
    InvalidPcid,
    NonCurrentPcidRequiresInvpcid,
}

/// Invalidates one translation in the current address space.
///
/// When INVPCID is available and a PCID is supplied, use the PCID-qualified
/// invalidation instead of INVLPG. This avoids relying on INVLPG behavior on
/// processors with documented PCID/global-entry errata.
pub fn invalidate_page(address: VirtAddr, pcid: Option<AddressSpacePcid>) -> Result<(), TlbInvalidationError> {
    let features = CpuFeatures::detect();

    if let Some(pcid) = pcid {
        if !features.invpcid() {
            return Err(TlbInvalidationError::PcidUnsupported);
        }

        let pcid = Pcid::new(pcid.raw()).map_err(|_| TlbInvalidationError::InvalidPcid)?;
        // SAFETY: INVPCID was confirmed by CPUID above and the PCID was
        // validated by the kernel's dedicated PCID type.
        unsafe { tlb::flush_pcid(InvPcidCommand::Address(address, pcid)); }
        return Ok(());
    }

    tlb::flush(address);
    Ok(())
}

/// Invalidates all non-global translations belonging to one PCID.
///
/// This operation is required before a retired PCID can ever be reused.
pub fn invalidate_pcid(pcid: AddressSpacePcid) -> Result<(), TlbInvalidationError> {
    if !CpuFeatures::detect().invpcid() {
        return Err(TlbInvalidationError::NonCurrentPcidRequiresInvpcid);
    }

    let pcid = Pcid::new(pcid.raw()).map_err(|_| TlbInvalidationError::InvalidPcid)?;
    // SAFETY: INVPCID was confirmed by CPUID above and this is the explicit
    // single-PCID invalidation command.
    unsafe { tlb::flush_pcid(InvPcidCommand::Single(pcid)); }
    Ok(())
}

/// Invalidates every TLB translation while using the architectural fallback.
/// This is deliberately a whole-CR3 flush and therefore a cold/exception path.
pub fn invalidate_all() {
    tlb::flush_all();
}

#[cfg(test)]
mod tests {
    use super::TlbInvalidationError;

    #[test]
    fn error_contract_distinguishes_missing_global_pcid_support() {
        assert_ne!(
            TlbInvalidationError::PcidUnsupported,
            TlbInvalidationError::NonCurrentPcidRequiresInvpcid,
        );
    }
}
