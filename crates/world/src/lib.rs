extern crate self as awrk_world;

pub use awrk_world_ecs::{
    Name, Parent, ProxyAuthority, ProxyAuthorityKind, ProxyEntity, ProxyLifecycle, ProxySpawnError,
    ProxySpawnRequest, ProxyState, RemoteParentRef, RemoteRef, WorldId,
};

pub mod bridge;
pub mod core;
pub mod registration;
pub mod rpc;
pub mod transport;

pub use bridge::{ProxySubscription, WorldBridge, WorldBridgeRemoteConfig};
pub use core::{Process, ProcessParts, Remotes, Rpcs, Sessions, World, WorldArgs, WorldEntityMut};
pub use inventory;
pub use rpc::{RpcTrace, WorldClient, WorldClientError, WorldClientOptions};

#[macro_export]
macro_rules! register_proxy_subscription {
    (
        all_of: [$($all_of:ty),* $(,)?],
        any_of: [$($any_of:ty),* $(,)?],
        none_of: [$($none_of:ty),* $(,)?],
        components: [$($component:ty),* $(,)?],
        outbound_create_components: [$($outbound_component:ty),* $(,)?] $(,)?
    ) => {
        const _: () = {
            fn build() -> $crate::registration::ProxySubscriptionContribution {
                $crate::registration::ProxySubscriptionContribution {
                    all_of: vec![$(::core::any::type_name::<$all_of>().to_string()),*],
                    any_of: vec![$(::core::any::type_name::<$any_of>().to_string()),*],
                    none_of: vec![$(::core::any::type_name::<$none_of>().to_string()),*],
                    components: vec![$(::core::any::type_name::<$component>().to_string()),*],
                    outbound_create_components: vec![$(::core::any::type_name::<$outbound_component>().to_string()),*],
                }
            }

        $crate::inventory::submit! {
            $crate::registration::ProxySubscriptionRegistration {
                build,
            }
        }
        };
    };
    (
        all_of: [$($all_of:ty),* $(,)?],
        any_of: [$($any_of:ty),* $(,)?],
        none_of: [$($none_of:ty),* $(,)?],
        components: [$($component:ty),* $(,)?] $(,)?
    ) => {
        $crate::register_proxy_subscription! {
            all_of: [$($all_of),*],
            any_of: [$($any_of),*],
            none_of: [$($none_of),*],
            components: [$($component),*],
            outbound_create_components: [$($component),*],
        }
    };
}
