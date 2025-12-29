/// Bridges Rust types to human-readable KDL schema terms.
///
/// This trait should be implemented for all primitive types and is automatically
/// implemented by `NodeDefinition` for configuration structs.
///
/// # Implementations
/// - `String` => "String"
/// - `u32` => "Integer"
/// - `bool` => "Boolean"
/// - Custom Structs => The KDL keyword or `schema_name` attribute value.
pub trait KdlSchemaType {
    const SCHEMA_NAME: &'static str;
}

macro_rules! impl_kdl_schema_type {
    ($($ty:ty => $name:expr),* $(,)?) => {
        $(
            impl KdlSchemaType for $ty {
                const SCHEMA_NAME: &'static str = $name;
            }
        )*
    };
}

impl_kdl_schema_type! {
    String => "String",
    str => "String",
    u8 => "Integer", u16 => "Integer", u32 => "Integer", u64 => "Integer", i128 => "Integer", usize => "Integer",
    i8 => "Integer", i16 => "Integer", i32 => "Integer", i64 => "Integer", u128 => "Integer", isize => "Integer",
    f32 => "Number", f64 => "Number",
    bool => "Boolean",
}
