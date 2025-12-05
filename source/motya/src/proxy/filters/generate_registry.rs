use motya_config::define_builtin_filters;
use motya_config::common_types::definitions::DefinitionsTable;
use crate::proxy::filters::registry::FilterRegistry;
use crate::proxy::filters::registry::RegistryFilterContainer;
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
use crate::proxy::filters::registry::FilterInstance;

macro_rules! impl_registry_loader {
    // ВАЖНО: Паттерн должен совпадать с тем, что в define_builtin_filters
    (
        // Actions
        actions: { $($act_key:literal => $act_type:ty),* $(,)? }
        
        // Requests
        requests: { $($req_key:literal => $req_type:ty),* $(,)? }

        // Responses
        responses: { $($res_key:literal => $res_type:ty),* $(,)? }
    ) => {
        pub fn load_registry(definitions: &mut DefinitionsTable) -> FilterRegistry {
            let mut registry = FilterRegistry::new();

            // 1. Actions
            $(
                let key = fqdn::fqdn!($act_key);
                definitions.insert_filter(key.clone()); // На всякий случай дублируем или проверяем
                
                registry.register_factory(key, Box::new(|settings| {
                    // Вот тут уже нужен реальный тип $act_type
                    let item = <$act_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Action(Box::new(item))))
                }));
            )*

            // 2. Requests
            $(
                let key = fqdn::fqdn!($req_key);
                definitions.insert_filter(key.clone());
                
                registry.register_factory(key, Box::new(|settings| {
                    let item = <$req_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Request(Box::new(item))))
                }));
            )*

            // 3. Responses
            $(
                let key = fqdn::fqdn!($res_key);
                definitions.insert_filter(key.clone());
                
                registry.register_factory(key, Box::new(|settings| {
                    let item = <$res_type>::from_settings(settings)?;
                    Ok(RegistryFilterContainer::Builtin(FilterInstance::Response(Box::new(item))))
                }));
            )*

            registry
        }
    };
}

// Вызываем каталог, передавая ему генератор реестра
define_builtin_filters!(impl_registry_loader);