#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePage<T> {
    pub(crate) records: Vec<T>,
    pub(crate) stored_bytes: usize,
    pub(crate) decoded_bytes: usize,
    pub(crate) has_more: bool,
}
