pub struct TypeRegistrar {
    pub register: fn(&mut crate::core::Process),
}

crate::inventory::collect!(TypeRegistrar);

pub fn register_discovered_types(process: &mut crate::core::Process) {
    for registrar in crate::inventory::iter::<TypeRegistrar> {
        (registrar.register)(process);
    }
}
