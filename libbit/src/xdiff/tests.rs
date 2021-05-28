use super::*;
use itertools::Itertools;

#[test]
fn test_xdiff_distance() {
    // wow this is terrible way to get to the required format
    let a = "ABCABBA".chars().map(|c| c.to_string()).collect_vec();
    let b = "CBABAC".chars().map(|c| c.to_string()).collect_vec();
    assert_eq!(
        xdiff_dist(
            a.iter().map(|s| s.as_str()).collect_vec().as_slice(),
            b.iter().map(|s| s.as_str()).collect_vec().as_slice()
        ),
        5
    );
}
