use northclock_core::{NorthclockError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TopologyRecordExtent {
    pub(crate) allocation_address: usize,
    pub(crate) allocation_size: usize,
    pub(crate) returned_size: usize,
    pub(crate) record_offset: usize,
    pub(crate) record_size: usize,
    pub(crate) minimum_record_size: usize,
    pub(crate) record_alignment: usize,
}

pub(crate) fn validate_topology_record_extent(extent: TopologyRecordExtent) -> Result<()> {
    if extent.record_alignment == 0 || !extent.record_alignment.is_power_of_two() {
        return invalid_topology_record_bounds();
    }
    if !extent
        .allocation_address
        .is_multiple_of(extent.record_alignment)
        || !extent.record_offset.is_multiple_of(extent.record_alignment)
        || !extent.record_size.is_multiple_of(extent.record_alignment)
        || extent.returned_size > extent.allocation_size
        || extent.record_size < extent.minimum_record_size
    {
        return invalid_topology_record_bounds();
    }
    let Some(record_address) = extent.allocation_address.checked_add(extent.record_offset) else {
        return invalid_topology_record_bounds();
    };
    if !record_address.is_multiple_of(extent.record_alignment) {
        return invalid_topology_record_bounds();
    }
    let Some(record_end) = extent.record_offset.checked_add(extent.record_size) else {
        return invalid_topology_record_bounds();
    };
    if record_end > extent.returned_size || record_end > extent.allocation_size {
        return invalid_topology_record_bounds();
    }
    Ok(())
}

fn invalid_topology_record_bounds() -> Result<()> {
    Err(NorthclockError::Internal(
        "invalid processor-topology record bounds".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_records_require_aligned_contained_extents() {
        let valid = TopologyRecordExtent {
            allocation_address: 0x1000,
            allocation_size: 96,
            returned_size: 80,
            record_offset: 32,
            record_size: 48,
            minimum_record_size: 8,
            record_alignment: 8,
        };
        assert!(validate_topology_record_extent(valid).is_ok());
        assert!(validate_topology_record_extent(TopologyRecordExtent {
            record_offset: 36,
            ..valid
        })
        .is_err());
        assert!(validate_topology_record_extent(TopologyRecordExtent {
            record_size: 56,
            ..valid
        })
        .is_err());
        assert!(validate_topology_record_extent(TopologyRecordExtent {
            returned_size: 104,
            ..valid
        })
        .is_err());
    }
}
