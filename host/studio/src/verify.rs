use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ByteMismatch {
    pub address: u32,
    pub expected: u8,
    pub actual: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffSummary {
    pub start_address: u32,
    pub compared_len: usize,
    pub mismatch_count: usize,
    pub mismatches: Vec<ByteMismatch>,
}

pub fn compute_diff(start_address: u32, expected: &[u8], actual: &[u8]) -> DiffSummary {
    let compared_len = expected.len().min(actual.len());
    let mut mismatches = Vec::new();

    for i in 0..compared_len {
        if expected[i] != actual[i] {
            mismatches.push(ByteMismatch {
                address: start_address + i as u32,
                expected: expected[i],
                actual: actual[i],
            });
        }
    }

    DiffSummary {
        start_address,
        compared_len,
        mismatch_count: mismatches.len(),
        mismatches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_no_mismatch() {
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 3];
        let diff = compute_diff(0x1000, &a, &b);
        assert_eq!(diff.mismatch_count, 0);
        assert_eq!(diff.compared_len, 3);
    }

    #[test]
    fn computes_mismatch_addresses() {
        let a = vec![0x10, 0x20, 0x30];
        let b = vec![0x10, 0x21, 0x30];
        let diff = compute_diff(0x2000, &a, &b);
        assert_eq!(diff.mismatch_count, 1);
        assert_eq!(diff.mismatches[0].address, 0x2001);
        assert_eq!(diff.mismatches[0].expected, 0x20);
        assert_eq!(diff.mismatches[0].actual, 0x21);
    }
}
