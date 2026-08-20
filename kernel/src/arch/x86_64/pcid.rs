//! Process-context identifier lifecycle primitives.
//!
//! PCID values are scoped to a logical CPU's TLB context. The allocator
//! distinguishes uncommitted reservations from active PCIDs so failed process
//! construction can return a never-used identifier without touching the TLB.
//! Committed PCIDs are not recycled until an architecture-level invalidation
//! protocol has retired their cached translations.

const PCID_MIN: u16 = 1;
const PCID_MAX: u16 = 4095;
const PCID_WORDS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpacePcid(u16);

impl AddressSpacePcid {
    pub const fn new(value: u16) -> Option<Self> {
        if value >= PCID_MIN && value <= PCID_MAX { Some(Self(value)) } else { None }
    }

    pub const fn raw(self) -> u16 { self.0 }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PcidLease(AddressSpacePcid);

impl PcidLease {
    pub const fn pcid(&self) -> AddressSpacePcid { self.0 }

    /// Commits the reservation to an execution context. Once committed, the
    /// PCID cannot be canceled and must eventually follow the retirement path.
    pub const fn commit(self) -> AddressSpacePcid { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcidAllocationError {
    Exhausted,
}

/// Conservative PCID allocator for the pre-SMP phase.
///
/// PCID 0 remains reserved for the non-PCID/default address-space context.
/// Committed identifiers are never silently recycled. Reuse will require an
/// architecture-level invalidation protocol, especially once multiple CPUs
/// may retain translations for the same identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcidAllocator {
    used: [u64; PCID_WORDS],
}

impl PcidAllocator {
    pub const fn new() -> Self {
        Self { used: [0; PCID_WORDS] }
    }

    /// Reserves a PCID for a construction transaction. The caller must either
    /// commit it into the execution context or cancel it before returning.
    pub fn reserve(&mut self) -> Result<PcidLease, PcidAllocationError> {
        for raw in PCID_MIN..=PCID_MAX {
            let index = raw as usize;
            let word = index / 64;
            let bit = index % 64;
            let mask = 1u64 << bit;
            if self.used[word] & mask == 0 {
                self.used[word] |= mask;
                return Ok(PcidLease(AddressSpacePcid(raw)));
            }
        }
        Err(PcidAllocationError::Exhausted)
    }

    /// Legacy immediate-allocation API. New construction code should prefer
    /// `reserve()` so failures can return an unused PCID without retirement.
    pub fn allocate(&mut self) -> Result<AddressSpacePcid, PcidAllocationError> {
        Ok(self.reserve()?.commit())
    }

    /// Cancels a reservation that has never been committed to execution.
    /// This is safe because no CR3/PCID translation was ever installed for it.
    pub fn cancel(&mut self, lease: PcidLease) {
        let pcid = lease.0;
        let index = pcid.raw() as usize;
        let word = index / 64;
        let bit = index % 64;
        self.used[word] &= !(1u64 << bit);
    }

    pub fn contains(&self, pcid: AddressSpacePcid) -> bool {
        let index = pcid.raw() as usize;
        let word = index / 64;
        let bit = index % 64;
        self.used[word] & (1u64 << bit) != 0
    }

    pub const fn capacity(&self) -> usize { (PCID_MAX - PCID_MIN + 1) as usize }
}

#[cfg(test)]
mod tests {
    use super::{AddressSpacePcid, PcidAllocationError, PcidAllocator};

    #[test]
    fn pcid_zero_is_reserved_and_bounds_are_strict() {
        assert_eq!(AddressSpacePcid::new(0), None);
        assert_eq!(AddressSpacePcid::new(1).unwrap().raw(), 1);
        assert_eq!(AddressSpacePcid::new(4095).unwrap().raw(), 4095);
        assert_eq!(AddressSpacePcid::new(4096), None);
    }

    #[test]
    fn allocator_starts_at_one_and_never_duplicates() {
        let mut allocator = PcidAllocator::new();
        let first = allocator.allocate().unwrap();
        let second = allocator.allocate().unwrap();
        assert_eq!(first.raw(), 1);
        assert_eq!(second.raw(), 2);
        assert!(allocator.contains(first));
        assert!(allocator.contains(second));
        assert_ne!(first, second);
    }

    #[test]
    fn canceled_reservation_can_be_reused_before_execution() {
        let mut allocator = PcidAllocator::new();
        let lease = allocator.reserve().unwrap();
        assert_eq!(lease.pcid().raw(), 1);
        allocator.cancel(lease);
        let reused = allocator.reserve().unwrap().commit();
        assert_eq!(reused.raw(), 1);
    }

    #[test]
    fn allocator_has_the_architectural_capacity() {
        let allocator = PcidAllocator::new();
        assert_eq!(allocator.capacity(), 4095);
    }

    #[test]
    fn allocator_reports_exhaustion_without_recycling() {
        let mut allocator = PcidAllocator::new();
        for expected in 1..=4095 {
            assert_eq!(allocator.allocate().unwrap().raw(), expected);
        }
        assert_eq!(allocator.allocate(), Err(PcidAllocationError::Exhausted));
    }
}
