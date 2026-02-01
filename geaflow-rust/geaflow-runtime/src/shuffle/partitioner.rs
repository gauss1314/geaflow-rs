pub fn partition_of_bytes(id: &[u8], partitions: usize) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut h = DefaultHasher::new();
    h.write(id);
    (h.finish() as usize) % partitions.max(1)
}
