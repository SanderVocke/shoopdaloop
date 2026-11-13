use crate::protocol::get;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub slot: u32,
    pub generation: u32,
    pub target: u32,
    pub ticks: bool,
}
impl Member {
    pub fn new(slot: u32, generation: u32, original: &[u32; 20]) -> Self {
        Self {
            slot,
            generation,
            target: get(original, 64, 32) as u32,
            ticks: get(original, 33, 1) != 0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub generation: u32,
    pub value: u32,
}
pub struct Membership {
    pub nodes: Vec<Node>,
    pub heads: Vec<[u32; 2]>,
    members: Vec<Member>,
}
impl Membership {
    pub fn new(loops: u32, requests: u32) -> Self {
        Self {
            nodes: vec![
                Node {
                    generation: 0,
                    value: NONE,
                };
                requests as usize
            ],
            heads: vec![[NONE, NONE]; loops as usize],
            members: Vec::with_capacity(requests as usize),
        }
    }
    pub fn metadata_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.heads.capacity() * std::mem::size_of::<[u32; 2]>()
            + self.members.capacity() * std::mem::size_of::<Member>()
    }
    pub fn rebuild(&mut self, members: impl Iterator<Item = Member>) -> Result<(), u8> {
        self.members.clear();
        let mut previous = None;
        for member in members {
            if member.slot as usize >= self.nodes.len()
                || member.target as usize >= self.heads.len()
                || member.generation == 0
                || previous.is_some_and(|slot| member.slot <= slot)
            {
                return Err(3);
            }
            previous = Some(member.slot);
            self.members.push(member);
        }
        self.nodes.fill(Node {
            generation: 0,
            value: NONE,
        });
        self.heads.fill([NONE, NONE]);
        for member in self.members.iter().rev() {
            let head = &mut self.heads[member.target as usize][usize::from(member.ticks)];
            self.nodes[member.slot as usize] = Node {
                generation: member.generation,
                value: *head,
            };
            *head = member.slot;
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.members.len()
    }
}
