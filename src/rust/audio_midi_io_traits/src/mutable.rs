use anyhow::Result;

pub trait Mutable {
    fn set_muted(self: &mut Self, muted: bool) -> Result<()>;
    fn get_muted(self: &Self) -> Result<bool>;
}
