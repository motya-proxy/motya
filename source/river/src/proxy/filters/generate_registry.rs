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



#[macro_export]
macro_rules! generate_registry {
    (
        fn $fn_name:ident;

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
        pub fn $fn_name() -> $crate::proxy::filters::registry::FilterRegistry {
            let mut registry = $crate::proxy::filters::registry::FilterRegistry::new();

            $($(
                registry.register_factory($action_key, Box::new(|settings| {
                    // Используем from_settings указанного типа
                    let item = <$action_type>::from_settings(settings)?;
                    Ok($crate::proxy::filters::registry::FilterInstance::Action(Box::new(item)))
                }));
            )*)?

            $($(
                registry.register_factory($req_key, Box::new(|settings| {
                    let item = <$req_type>::from_settings(settings)?;
                    Ok($crate::proxy::filters::registry::FilterInstance::Request(Box::new(item)))
                }));
            )*)?

            $($(
                registry.register_factory($res_key, Box::new(|settings| {
                    let item = <$res_type>::from_settings(settings)?;
                    Ok($crate::proxy::filters::registry::FilterInstance::Response(Box::new(item)))
                }));
            )*)?

            registry
        }
    };
}


generate_registry! {
    fn load_registry;

    actions: {
        "river.filters.block-cidr-range" => CidrRangeFilter,
    }

    requests: {
        "river.request.upsert-header" => RequestUpsertHeader,
        "river.request.remove-header" => RequestRemoveHeaderKeyRegex,
    }

    responses: {
        "river.response.upsert-header" => ResponseUpsertHeader,
        "river.response.remove-header" => ResponseRemoveHeaderKeyRegex,
    }
}
