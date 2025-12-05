use crate::proxy::filters::builtin::{
    cidr_range::CidrRangeFilter, request::{
        remove_headers::RemoveHeaderKeyRegex as RequestRemoveHeaderKeyRegex, 
        upsert_headers::UpsertHeader as RequestUpsertHeader
    }, 
    response::{
        remove_header::RemoveHeaderKeyRegex as ResponseRemoveHeaderKeyRegex, 
        upsert_header::UpsertHeader as ResponseUpsertHeader
    }
};


macro_rules! generate_registry {
    (
        fn $fn_name:ident;
        fn $fn_name2:ident;

        $(
            actions: {
                $($action_key:literal => $action_type:ty),* $(,)?
            }
        )?

        $(
            requests: {
                $($req_key:literal => $req_type:ty),* $(,)?
            }
        )?

        $(
            responses: {
                $($res_key:literal => $res_type:ty),* $(,)?
            }
        )?
    ) => {
        /// Registers all built-in (native) filters into the registry.
        ///
        /// These filters are compiled directly into the binary. For implementation details
        /// and the list of available filters, refer to the [`proxy::filters::builtin`] module
        pub fn $fn_name(
            definitions: &mut $crate::config::common_types::definitions::DefinitionsTable
        ) -> $crate::proxy::filters::registry::FilterRegistry {
            let mut registry = $crate::proxy::filters::registry::FilterRegistry::new();
            use std::str::FromStr;
            use $crate::proxy::filters::registry::{RegistryFilterContainer, FilterInstance};
            $($(
                let action_key = fqdn::FQDN::from_str($action_key).expect("not valid FQDN");
                definitions.insert_filter(action_key.clone());

                registry.register_factory(action_key, Box::new(|settings| {
                    let item = <$action_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Action(Box::new(item))))
                }));
            )*)?

            $($(
                let req_key = fqdn::FQDN::from_str($req_key).expect("not valid FQDN");
                definitions.insert_filter(req_key.clone());
                registry.register_factory(req_key, Box::new(|settings| {
                    let item = <$req_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Request(Box::new(item))))
                }));
            )*)?

            $($(
                let res_key = fqdn::FQDN::from_str($res_key).expect("not valid FQDN");
                definitions.insert_filter(res_key.clone());
                registry.register_factory(res_key, Box::new(|settings| {
                    let item = <$res_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Response(Box::new(item))))
                }));
            )*)?

            registry
        }

        pub fn $fn_name2() -> $crate::config::common_types::definitions::DefinitionsTable {
            let mut definitions: $crate::config::common_types::definitions::DefinitionsTable = Default::default();
            $($(
                let action_key = fqdn::fqdn!($action_key);
                definitions.insert_filter(action_key.clone());
            )*)?

            $($(
                let req_key = fqdn::fqdn!($req_key);
                definitions.insert_filter(req_key.clone());
            )*)?

            $($(
                let res_key = fqdn::fqdn!($res_key);
                definitions.insert_filter(res_key.clone());
            )*)?

            definitions
        }
    };
}


generate_registry! {
    fn load_registry;
    fn load_definitions_table;

    actions: {
        "motya.filters.block-cidr-range" => CidrRangeFilter,
    }

    requests: {
        "motya.request.upsert-header" => RequestUpsertHeader,
        "motya.request.remove-header" => RequestRemoveHeaderKeyRegex,
    }

    responses: {
        "motya.response.upsert-header" => ResponseUpsertHeader,
        "motya.response.remove-header" => ResponseRemoveHeaderKeyRegex,
    }
}
