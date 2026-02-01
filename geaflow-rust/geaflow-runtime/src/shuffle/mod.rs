pub mod partitioner;

use std::collections::HashMap;

pub type Outbox = Vec<(Vec<u8>, Vec<u8>)>;
pub type Inbox = HashMap<Vec<u8>, Vec<Vec<u8>>>;
pub type Inboxes = Vec<Inbox>;

pub trait MessageShuffle: Send + Sync {
    fn route_outbox(&self, outbox: Outbox, partitions: usize, next_inboxes: &mut Inboxes);
}

pub struct DriverShuffle;

impl MessageShuffle for DriverShuffle {
    fn route_outbox(&self, outbox: Outbox, partitions: usize, next_inboxes: &mut Inboxes) {
        for (target, msg) in outbox {
            let p = partitioner::partition_of_bytes(&target, partitions);
            next_inboxes[p].entry(target).or_default().push(msg);
        }
    }
}
