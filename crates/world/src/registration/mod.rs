pub struct TypeRegistrar {
    pub register: fn(&mut crate::core::Process),
}

pub struct ProxySubscriptionContribution {
    pub all_of: Vec<String>,
    pub any_of: Vec<String>,
    pub none_of: Vec<String>,
    pub components: Vec<String>,
    pub outbound_create_components: Vec<String>,
}

pub struct ProxySubscriptionRegistration {
    pub build: fn() -> ProxySubscriptionContribution,
}

crate::inventory::collect!(TypeRegistrar);
crate::inventory::collect!(ProxySubscriptionRegistration);

pub fn register_discovered(process: &mut crate::core::Process) {
    for registrar in crate::inventory::iter::<TypeRegistrar> {
        (registrar.register)(process);
    }
}
