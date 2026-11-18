const HEADER_BYTES: usize = 40;

/// The voxel count a `Terrain.terrain` blob declares in its header.
///
/// The product of the three little-endian `i32` dimensions at offsets 0, 4 and 8. `None` when the
/// header is shorter than the 40 bytes the plugin writes, a dimension is negative, or the product
/// does not fit a `usize`.
pub fn terrain_voxel_count(content: &[u8]) -> Option<usize> {
    if content.len() < HEADER_BYTES {
        return None;
    }
    let dimensions = [content.get(0..4), content.get(4..8), content.get(8..12)];
    let count = dimensions.into_iter().try_fold(1_u64, |count, bytes| {
        let dimension = u64::try_from(i32::from_le_bytes(bytes?.try_into().ok()?)).ok()?;
        count.checked_mul(dimension)
    })?;
    usize::try_from(count).ok()
}

#[cfg(test)]
mod tests {
    use super::terrain_voxel_count;

    fn blob(nx: i32, ny: i32, nz: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        for dimension in [nx, ny, nz] {
            bytes.extend_from_slice(&dimension.to_le_bytes());
        }
        bytes.resize(40, 0);
        bytes
    }

    #[test]
    fn the_voxel_count_is_the_product_of_the_declared_dimensions() {
        assert_eq!(terrain_voxel_count(&blob(4, 8, 16)), Some(512));
        assert_eq!(terrain_voxel_count(&blob(2, 3, 5)), Some(30));
    }

    #[test]
    fn a_negative_dimension_has_no_voxel_count() {
        assert_eq!(terrain_voxel_count(&blob(4, -1, 4)), None);
    }

    #[test]
    fn a_header_shorter_than_the_plugin_header_has_no_voxel_count() {
        let mut short = blob(2, 2, 2);
        short.truncate(39);
        assert_eq!(terrain_voxel_count(&short), None);
    }

    #[test]
    fn an_overflowing_product_has_no_voxel_count() {
        assert_eq!(
            terrain_voxel_count(&blob(i32::MAX, i32::MAX, i32::MAX)),
            None
        );
    }
}
