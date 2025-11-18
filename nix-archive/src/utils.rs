pub mod log;

pub const PADDING: usize = 8;

#[inline]
pub fn calculate_padding(strlen: u64) -> usize {
    (PADDING - (strlen % PADDING as u64) as usize) % PADDING
}
