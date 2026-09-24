#[decl_rils(core::collections)]
mod native {
    #[rils_struct]
    pub struct Buffer;

    #[rils_enum(id_prefix = core::old_state)]
    pub enum State {
        Idle,
    }

    #[rils_trait]
    pub trait Marker: super::Marker {}

    #[rils_derive(Marker)]
    fn derive_marker() {}

    pub struct Helper;
}
