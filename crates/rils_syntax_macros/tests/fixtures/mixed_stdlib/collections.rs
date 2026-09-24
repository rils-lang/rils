#[decl_rils(core::collections)]
mod native {
    #[rils_struct]
    pub struct Buffer;

    #[rils_enum]
    pub enum State {
        Idle,
    }

    #[rils_trait]
    pub trait Marker: super::Marker {}

    #[rils_derive(Marker)]
    fn derive_marker() {}

    pub struct Helper;
}
