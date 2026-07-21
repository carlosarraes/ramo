pub(super) fn wrapping_index(current: usize, len: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let current = current.min(len - 1) as i64;
    let delta = i64::from(delta);
    Some((current + delta).rem_euclid(len as i64) as usize)
}

pub(super) fn signed_offset(value: usize, delta: i32, amount: usize) -> usize {
    let distance = amount.saturating_mul(delta.unsigned_abs() as usize);
    if delta.is_negative() {
        value.saturating_sub(distance)
    } else {
        value.saturating_add(distance)
    }
}

#[cfg(test)]
mod tests {
    use super::{signed_offset, wrapping_index};

    #[test]
    fn wrapping_and_saturating_navigation_are_total() {
        assert_eq!(wrapping_index(0, 0, 1), None);
        assert_eq!(wrapping_index(0, 3, -1), Some(2));
        assert_eq!(wrapping_index(2, 3, 1), Some(0));
        assert_eq!(signed_offset(2, -1, 5), 0);
        assert_eq!(signed_offset(2, 1, 5), 7);
    }
}
