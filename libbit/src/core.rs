/// same as `std::cmp::Ord` but without the requirements of consistency with `std::cmp::Eq` and `std::cmp::PartialOrd`
pub trait BitOrd {
    fn bit_cmp(&self, other: &Self) -> std::cmp::Ordering;
}
